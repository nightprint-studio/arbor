//! Finding a vault, opening it, creating one, and listing the notes in it.
//!
//! ## A vault is a folder somebody pointed at
//!
//! There is no registry to consult and no magic. [`Vault::open`] takes the folder
//! the user picked; [`find_upward`] exists for the other entry point — a file
//! dropped on the window, a deep link, a path from the command line — where the
//! path names a note and the vault is somewhere above it.
//!
//! The marker is the `.arbor/garrulus/` directory. Its presence is the whole
//! definition: a folder of markdown without one is a folder of markdown, and
//! [`Vault::create`] is what turns it into a vault, on purpose, once.
//!
//! ## The scan is thin, everything else is pure
//!
//! [`scan_notes`] is the only function here that touches a filesystem in bulk,
//! and all it produces is a sorted list of paths. Deciding what those paths
//! *mean* — which type, which title, which links — is [`crate::note`] and
//! [`crate::note_type`], neither of which needs a directory to be tested. That
//! split is the reason the interesting half of this crate has unit tests instead
//! of fixtures.

use std::path::{Path, PathBuf};

use crate::builtin::install_builtin_types;
use crate::config::{marker_dir, VaultConfig};
use crate::error::{VaultError, VaultResult};
use crate::note::Note;
use crate::note_type::{classify, load_types, NoteType, TypeId};
use crate::path::{glob_matches, to_rel, RelPath};

/// An open vault: where it is, what it has decided about itself, and what note
/// types it knows.
///
/// Not a handle and not a cache — three fields read at open. The notes are not in
/// here on purpose: they belong to the index, which is rebuildable, whereas this
/// is the small amount of state that is genuinely the vault's.
#[derive(Debug, Clone)]
pub struct Vault {
    /// The absolute path of the vault root.
    pub root: PathBuf,
    /// `.arbor/garrulus/vault.toml`, or the defaults when it says nothing.
    pub config: VaultConfig,
    /// The types under `.arbor/garrulus/types/`, sorted by id.
    pub types: Vec<NoteType>,
}

impl Vault {
    /// Is there a vault here?
    pub fn is_vault(root: &Path) -> bool {
        marker_dir(root).is_dir()
    }

    /// Open the vault at `root`.
    ///
    /// Returns the type files that would not parse alongside the vault rather
    /// than instead of it: one broken type must never be the reason a user cannot
    /// reach their notes.
    pub fn open(root: &Path) -> VaultResult<(Vault, Vec<VaultError>)> {
        if !root.is_dir() {
            return Err(VaultError::NotADirectory { path: root.to_path_buf() });
        }
        if !Vault::is_vault(root) {
            return Err(VaultError::NotAVault { path: root.to_path_buf() });
        }
        let config = VaultConfig::load(root)?.unwrap_or_default();
        let (types, problems) = load_types(root)?;
        Ok((Vault { root: root.to_path_buf(), config, types }, problems))
    }

    /// Turn a folder into a vault: write the marker, the settings and the seven
    /// shipped types.
    ///
    /// The folder may already be full of markdown — that is the *expected* case,
    /// because the migration from an existing Obsidian vault is this call and
    /// nothing else. It may not already be a vault: rewriting `vault.toml` and
    /// the types over the top of ones the user has edited is not a create.
    pub fn create(root: &Path, name: &str) -> VaultResult<Vault> {
        if !root.is_dir() {
            return Err(VaultError::NotADirectory { path: root.to_path_buf() });
        }
        if Vault::is_vault(root) {
            return Err(VaultError::AlreadyAVault { path: root.to_path_buf() });
        }
        let dir = marker_dir(root);
        std::fs::create_dir_all(&dir).map_err(|e| VaultError::io(&dir, e))?;

        let display = if name.trim().is_empty() { folder_name(root) } else { name.trim().into() };
        let mut config = VaultConfig { name: display, ..VaultConfig::default() };
        config.save(root)?;
        install_builtin_types(root)?;

        let (types, _) = load_types(root)?;
        Ok(Vault { root: root.to_path_buf(), config, types })
    }

    /// `<root>/.arbor/garrulus`.
    pub fn marker_dir(&self) -> PathBuf {
        marker_dir(&self.root)
    }

    /// Where pasted images land.
    pub fn attachments_dir(&self) -> PathBuf {
        RelPath::new(&self.config.attachments).to_path(&self.root)
    }

    /// The type with this id.
    pub fn note_type(&self, id: &TypeId) -> Option<&NoteType> {
        self.types.iter().find(|candidate| &candidate.id == id)
    }

    /// The type with this id, or a named error — for the call sites where a
    /// missing type is a failed request rather than an ordinary absence.
    pub fn require_type(&self, id: &TypeId) -> VaultResult<&NoteType> {
        self.note_type(id).ok_or_else(|| VaultError::UnknownType { id: id.to_string() })
    }

    /// Which type is this note? Delegates to [`classify`] with the vault's own
    /// types, so no call site has to remember to pass them.
    pub fn classify(&self, note: &Note) -> Option<TypeId> {
        classify(note, &self.types)
    }

    /// Resolve a vault-relative path.
    pub fn absolute(&self, path: &RelPath) -> PathBuf {
        path.to_path(&self.root)
    }

    /// Express an absolute path as a vault-relative one, or `None` when it is not
    /// in this vault. The gate every path arriving from the outside — a file
    /// watcher event, a drop, a deep link — has to pass.
    pub fn relative(&self, absolute: &Path) -> Option<RelPath> {
        to_rel(&self.root, absolute)
    }

    /// Where a new note of this type goes, name included.
    ///
    /// The filename is not deduplicated here: whether a colliding title should
    /// become `Crash 2` or should reopen the existing note is a decision for the
    /// caller, and [`crate::naming::unique_name`] is the tool for the first
    /// answer.
    pub fn new_note_path(&self, note_type: &NoteType, file_name: &str) -> RelPath {
        RelPath::new(&note_type.folder).join(file_name)
    }

    /// Every note in the vault, sorted.
    pub fn notes(&self) -> VaultResult<Vec<RelPath>> {
        scan_notes(&self.root, &self.config.excluded)
    }
}

/// The folder's own name, which is the vault's suggested display name.
fn folder_name(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Walk a vault and return every note in it, sorted by path.
///
/// Dot-directories are skipped unconditionally — `.git`, `.arbor`, `.obsidian`,
/// `.trash` — and so `excluded` is for the user's own folders, not for the
/// plumbing. Symlinks are not followed: a vault that syncs between machines must
/// not index a folder that only exists on one of them, and a link pointing at an
/// ancestor is an infinite walk.
///
/// Written against `std::fs` rather than `arbor_fs::read_dir` because the whole
/// point is to prune mid-traversal, which a flat listing cannot express.
pub fn scan_notes(root: &Path, excluded: &[String]) -> VaultResult<Vec<RelPath>> {
    if !root.is_dir() {
        return Err(VaultError::NotADirectory { path: root.to_path_buf() });
    }

    let mut out = Vec::new();
    // An explicit stack, not recursion: a pathological depth in somebody's vault
    // must not be able to blow the backend's stack.
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            // An unreadable folder is skipped: a permissions problem on one
            // subfolder is not a reason to fail opening the vault.
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let Some(relative) = to_rel(root, &path) else { continue };
            if is_excluded(&relative, excluded) {
                continue;
            }
            if kind.is_dir() {
                pending.push(path);
            } else if relative.is_note() {
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Does any exclusion glob cover this path?
pub fn is_excluded(path: &RelPath, excluded: &[String]) -> bool {
    excluded.iter().any(|glob| glob_matches(glob, path.as_str()))
}

/// Walk up from `start` looking for the `.arbor/garrulus/` marker.
///
/// For the entry points that name a note rather than a vault: a dropped file, a
/// deep link, a path off the command line. `start` may be a file or a folder.
pub fn find_upward(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() { Some(start) } else { start.parent() };
    while let Some(dir) = current {
        if Vault::is_vault(dir) {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_is_by_glob_over_the_relative_path() {
        let excluded = vec!["archivio/**".to_string(), "*.draft.md".to_string()];
        assert!(is_excluded(&RelPath::new("archivio/2019/x.md"), &excluded));
        assert!(is_excluded(&RelPath::new("x.draft.md"), &excluded));
        assert!(!is_excluded(&RelPath::new("bugs/x.md"), &excluded));
        assert!(!is_excluded(&RelPath::new("bugs/x.md"), &[]));
    }

    #[test]
    fn a_new_note_lands_in_its_types_folder() {
        let vault = Vault {
            root: PathBuf::from("/vault"),
            config: VaultConfig::default(),
            types: Vec::new(),
        };
        let mut bug = NoteType::new("bug", "Bug");
        bug.folder = "bugs".into();
        assert_eq!(vault.new_note_path(&bug, "Crash.md").as_str(), "bugs/Crash.md");

        // A type that files at the root produces a top-level path, not `/Crash`.
        let loose = NoteType::new("loose", "Nota");
        assert_eq!(vault.new_note_path(&loose, "Crash.md").as_str(), "Crash.md");
    }

    #[test]
    fn a_missing_type_is_a_named_failure_not_a_none() {
        let vault = Vault {
            root: PathBuf::from("/vault"),
            config: VaultConfig::default(),
            types: vec![NoteType::new("bug", "Bug")],
        };
        assert!(vault.require_type(&TypeId::new("bug")).is_ok());
        let message = vault.require_type(&TypeId::new("epic")).unwrap_err().to_string();
        assert_eq!(message, "there is no note type called `epic`");
    }
}
