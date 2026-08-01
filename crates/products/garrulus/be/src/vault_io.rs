//! `vault_io` — the one place this binary touches the vault on disk.
//!
//! Two jobs, deliberately together:
//!
//! 1. **The seam onto `garrulus-vault`'s I/O.** Opening a vault, scanning it, and
//!    re-reading one note into a [`Note`] are the only calls the handlers make
//!    into the vault crate's *stateful* half. They are wrapped here so a change in
//!    that crate's shape is a change to four functions rather than to nine
//!    handlers. (This is the same containment rule `garrulus-parse` applies to its
//!    tree-sitter touchpoints.)
//! 2. **Path guarding.** Every note-addressing handler takes a vault-relative path
//!    that came over IPC from the frontend, and every one of them must be unable
//!    to reach outside the vault. That check is [`resolve_rel`], it is pure, and it
//!    is tested — there is exactly one implementation of it in this backend.
//!
//! Nothing here holds a lock: [`with_vault`] takes the vault's read guard, runs
//! the closure, and drops it before returning, so a caller can then take the index
//! lock without ever nesting the two (see `garrulus_core::state`'s lock order).

use std::path::{Path, PathBuf};

use garrulus_core::prelude::{
    read_note, trash_dir as vault_trash_dir, GarrulusState, Note, RelPath, Vault,
    MARKER_RELATIVE_PATH,
};

// ── The seam onto garrulus-vault ──────────────────────────────────────────────

/// Open an existing vault: read its marker folder, its settings and its note
/// types. Fails when `root` is not a vault.
///
/// A type file that would not parse is reported on stderr rather than failing the
/// open — the vault crate hands those back precisely so one broken type is never
/// the reason a user cannot reach their notes.
pub fn open_vault(root: &Path) -> Result<Vault, String> {
    let (vault, problems) = Vault::open(root).map_err(|e| e.to_string())?;
    for problem in &problems {
        eprintln!("garrulus: tipo di nota non caricato: {problem}");
    }
    Ok(vault)
}

/// Create a vault at `root`: the marker folder, the default settings and the
/// built-in note types. Fails when one is already there.
///
/// `name` may be empty — the vault crate then falls back to the folder's own name.
pub fn create_vault(root: &Path, name: &str) -> Result<Vault, String> {
    Vault::create(root, name).map_err(|e| e.to_string())
}

/// Read and parse every note in the vault — the input to an index build. Called
/// at vault open and by an explicit rebuild, never per keystroke.
///
/// A note that will not parse is **skipped, not fatal**: one malformed file must
/// not leave the user with no index at all. The failures go to stderr, and the
/// problems panel is where they belong once it is wired.
pub fn scan_notes(vault: &Vault) -> Result<Vec<Note>, String> {
    let paths = vault.notes().map_err(|e| e.to_string())?;
    let mut notes = Vec::with_capacity(paths.len());
    for path in &paths {
        match read_note(&vault.root, path, &vault.types) {
            Ok(note) => notes.push(note),
            Err(e) => eprintln!("garrulus: nota non letta ({}): {e}", path.as_str()),
        }
    }
    Ok(notes)
}

/// Read and parse one note, addressed vault-relative. The per-save index upsert
/// goes through here.
pub fn load_note(vault: &Vault, rel: &str) -> Result<Note, String> {
    read_note(&vault.root, &RelPath::new(rel), &vault.types).map_err(|e| e.to_string())
}

/// Run `f` against the open vault, dropping the read guard before returning.
///
/// The canonical opening move of a handler that needs anything the vault knows:
/// it keeps the guard's lifetime to a single statement, which is what makes the
/// "never nest vault and index" rule easy to follow rather than a discipline.
pub fn with_vault<T>(
    state: &GarrulusState,
    f: impl FnOnce(&Vault) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state.vault_read()?;
    let vault = guard.as_ref().ok_or_else(|| "no vault is open".to_string())?;
    f(vault)
}

// ── Note file I/O ─────────────────────────────────────────────────────────────

/// Read a note's source text, verbatim. No parsing: the editor round-trips bytes,
/// and the frontmatter round-trip invariant depends on nothing rewriting them.
pub fn read_source(root: &Path, rel: &str) -> Result<String, String> {
    let path = resolve_rel(root, rel)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{rel}: {e}"))
}

/// Write a note's source text, creating the parent folder if the note is being
/// filed somewhere new.
pub fn write_source(root: &Path, rel: &str, text: &str) -> Result<(), String> {
    let path = resolve_rel(root, rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{rel}: {e}"))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{rel}: {e}"))
}

/// Whether a vault-relative path already exists (a note, or anything else).
pub fn exists(root: &Path, rel: &str) -> Result<bool, String> {
    Ok(resolve_rel(root, rel)?.exists())
}

// ── Paths ─────────────────────────────────────────────────────────────────────

/// Turn a vault-relative path from the frontend into an absolute one, refusing
/// anything that could address a file outside the vault.
///
/// Rejects the empty path, absolute paths, Windows drive prefixes and any `..`
/// segment. Deliberately a **syntactic** check performed before touching the
/// filesystem: it does not resolve symlinks, so a symlink planted inside the vault
/// can still point out of it — which is the user's own vault doing what they told
/// it to, not an untrusted input crossing the seam.
pub fn resolve_rel(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let cleaned = rel.replace('\\', "/");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return Err("empty note path".to_string());
    }
    if trimmed.starts_with('/') || trimmed.contains(':') {
        return Err(format!("{rel}: note paths are relative to the vault"));
    }
    let mut out = root.to_path_buf();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err(format!("{rel}: a note path cannot leave the vault")),
            s => out.push(s),
        }
    }
    if out == root {
        return Err(format!("{rel}: not a note path"));
    }
    Ok(out)
}

/// The vault-relative, POSIX-separated form of an absolute path inside the vault,
/// or `None` when the path is outside it. Used by the watcher to report what
/// changed in the same vocabulary the handlers take.
pub fn to_rel(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    let text = rel.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Whether a vault-relative path is Garrulus's own project state
/// (`.arbor/garrulus/…`) or the vault's git directory — the two things a change
/// event must not report as "a note changed".
pub fn is_internal(rel: &str) -> bool {
    // The first segment of the marker path — `.arbor` — rather than the literal,
    // so this stays true if the product's dot-folder ever moves.
    let dot = MARKER_RELATIVE_PATH.split('/').next().unwrap_or(".arbor");
    let rel = rel.trim_start_matches("./");
    rel.starts_with(&format!("{dot}/"))
        || rel == dot
        || rel.starts_with(".git/")
        || rel == ".git"
}

/// Whether a vault-relative path names a note (as opposed to an attachment).
///
/// Delegates to the vault's own definition — three spellings of "is this a note"
/// is how a `.markdown` file ends up mirrored by the sync engine and invisible to
/// the index.
pub fn is_note(rel: &str) -> bool {
    RelPath::new(rel).is_note()
}

/// The trash folder inside the vault, created on demand by the delete handler.
pub fn trash_dir(root: &Path) -> PathBuf {
    vault_trash_dir(root)
}

/// Unix milliseconds, for the trash entry's name and the vault registry's
/// last-opened stamp. `0` if the system clock predates the epoch, which is not a
/// case worth an error path.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/vault")
    }

    #[test]
    fn resolve_rel_accepts_ordinary_note_paths() {
        assert_eq!(resolve_rel(&root(), "note.md").unwrap(), root().join("note.md"));
        assert_eq!(
            resolve_rel(&root(), "bugs/2026-07-31-crash.md").unwrap(),
            root().join("bugs").join("2026-07-31-crash.md")
        );
        // Windows separators from the frontend are normalised, not rejected.
        assert_eq!(
            resolve_rel(&root(), "bugs\\crash.md").unwrap(),
            root().join("bugs").join("crash.md")
        );
        // A `.` segment is noise, not an escape.
        assert_eq!(resolve_rel(&root(), "./note.md").unwrap(), root().join("note.md"));
    }

    #[test]
    fn resolve_rel_refuses_to_leave_the_vault() {
        for bad in ["../secrets.md", "bugs/../../secrets.md", "/etc/passwd", "C:/notes/x.md"] {
            assert!(resolve_rel(&root(), bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn resolve_rel_refuses_empty_and_root() {
        assert!(resolve_rel(&root(), "").is_err());
        assert!(resolve_rel(&root(), "   ").is_err());
        assert!(resolve_rel(&root(), ".").is_err(), "the vault root is not a note");
    }

    #[test]
    fn to_rel_is_posix_and_scoped() {
        assert_eq!(to_rel(&root(), &root().join("a").join("b.md")), Some("a/b.md".to_string()));
        assert_eq!(to_rel(&root(), Path::new("/elsewhere/b.md")), None);
        assert_eq!(to_rel(&root(), &root()), None, "the root itself is not a note path");
    }

    #[test]
    fn internal_paths_are_not_note_changes() {
        assert!(is_internal(".arbor/garrulus/vault.toml"));
        assert!(is_internal(".git/index"));
        assert!(!is_internal("notes/.arbor-ish.md"));
        assert!(!is_internal("bugs/crash.md"));
    }

    #[test]
    fn note_detection_is_extension_only_and_case_insensitive() {
        assert!(is_note("a/b.md"));
        assert!(is_note("a/b.MD"));
        assert!(!is_note("a/b.png"));
        assert!(!is_note("a/b"));
    }
}
