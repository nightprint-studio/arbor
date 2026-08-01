//! The vault trash — deleting a note without deleting it.
//!
//! Deleting a note is scary in a way deleting a file is not: a file you can
//! usually get back from somewhere, whereas a note is the only copy of a thought
//! you had once. So a delete here is a **move** into
//! `<vault>/.arbor/garrulus/trash/`, with a sidecar recording where the note came
//! from, and getting it back is one click rather than an expedition into git
//! history.
//!
//! Only [`purge`] and [`empty`] actually remove anything, and they hand the file
//! to the operating system's own trash rather than unlinking it — so even the
//! deliberate, confirmed, second delete is recoverable.
//!
//! ## Why a `.toml` sidecar and not a filename convention
//!
//! The original path has to survive, and folding it into the file name would
//! mean encoding `/` into something, decoding it on the way back, and getting it
//! wrong for a note whose title already contains the escape character. A
//! two-line TOML file next to the note is boring, readable in a text editor, and
//! merges as metadata rather than as content when the trash itself syncs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::trash_dir;
use crate::error::{VaultError, VaultResult};
use crate::naming::sanitize_file_name;
use crate::path::{path_str, RelPath};

/// One note waiting in the trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TrashedNote {
    /// The entry's id — the stem of both files inside `trash/`.
    pub id: String,
    /// Where the note was when it was deleted, and where [`restore`] puts it
    /// back.
    pub original: RelPath,
    /// When it was trashed, as the caller's timestamp string. This crate does not
    /// read a clock; see [`crate::template`].
    pub trashed_at: String,
    /// What the note was called, so the trash list is readable without opening
    /// anything.
    pub title: String,
}

/// The id of a trash entry: the note's name plus the moment it was deleted.
///
/// The stamp is part of the id rather than only of the sidecar because two notes
/// called `2026-07-31.md` from two different folders will otherwise collide, and
/// the second delete would overwrite the first — which is exactly the failure
/// this module exists to prevent.
pub fn entry_id(path: &RelPath, stamp: &str) -> String {
    let stem = sanitize_file_name(path.stem());
    let stamp = sanitize_file_name(stamp);
    let stem = if stem.is_empty() { "nota".to_string() } else { stem };
    if stamp.is_empty() {
        stem
    } else {
        format!("{stem}--{stamp}")
    }
}

/// The two files an entry owns: the note itself and its sidecar.
fn entry_paths(root: &Path, id: &str) -> (PathBuf, PathBuf) {
    let dir = trash_dir(root);
    (dir.join(format!("{id}.md")), dir.join(format!("{id}.toml")))
}

/// Move a note into the trash.
///
/// `stamp` is the caller's timestamp — anything sortable and filename-safe; the
/// convention is `yyyy-MM-dd-HHmm`, which is what makes the trash list read in
/// order without parsing anything.
pub fn trash_note(
    root: &Path,
    path: &RelPath,
    title: &str,
    stamp: &str,
) -> VaultResult<TrashedNote> {
    if path.escapes() {
        return Err(VaultError::BadPath {
            raw: path.as_str().to_string(),
            reason: "it points outside the vault".to_string(),
        });
    }
    let source = path.to_path(root);
    if !source.is_file() {
        return Err(VaultError::NoteMissing { path: path.clone() });
    }

    let dir = trash_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| VaultError::io(&dir, e))?;

    let mut id = entry_id(path, stamp);
    let (mut note_file, mut sidecar) = entry_paths(root, &id);
    // Two deletes inside the same minute are rare and are still not allowed to
    // eat each other.
    let mut attempt = 2;
    while note_file.exists() || sidecar.exists() {
        id = format!("{}-{attempt}", entry_id(path, stamp));
        (note_file, sidecar) = entry_paths(root, &id);
        attempt += 1;
    }

    let entry = TrashedNote {
        id,
        original: path.clone(),
        trashed_at: stamp.to_string(),
        title: title.to_string(),
    };
    let text = toml::to_string_pretty(&entry).map_err(|e| VaultError::malformed(&sidecar, e))?;
    std::fs::write(&sidecar, text).map_err(|e| VaultError::io(&sidecar, e))?;

    // The note moves last: if the rename fails, the vault still has the note and
    // the only debris is a sidecar the next `list` will report as an orphan.
    std::fs::rename(&source, &note_file).map_err(|e| VaultError::io(&source, e))?;
    Ok(entry)
}

/// Everything currently in the trash, newest-looking id last.
///
/// A sidecar whose note is missing is skipped rather than reported: it is debris
/// from an interrupted delete, and there is nothing the user can do about it.
pub fn list(root: &Path) -> VaultResult<Vec<TrashedNote>> {
    let dir = trash_dir(root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| VaultError::io(&dir, e))?;

    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| VaultError::io(&dir, e))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(trashed) = toml::from_str::<TrashedNote>(&text) else { continue };
        if entry_paths(root, &trashed.id).0.is_file() {
            out.push(trashed);
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Put a note back where it came from.
///
/// Refuses when something is already at the original path — restoring over a
/// note written since the delete would lose the newer one, and the caller has a
/// better answer available (ask, or restore under a fresh name).
pub fn restore(root: &Path, id: &str) -> VaultResult<RelPath> {
    let (note_file, sidecar) = entry_paths(root, id);
    if !note_file.is_file() {
        return Err(VaultError::NoteMissing { path: RelPath::new(id) });
    }
    let text = std::fs::read_to_string(&sidecar).map_err(|e| VaultError::io(&sidecar, e))?;
    let entry: TrashedNote =
        toml::from_str(&text).map_err(|e| VaultError::malformed(&sidecar, e))?;

    let target = entry.original.to_path(root);
    if target.exists() {
        return Err(VaultError::NoteExists { path: entry.original.clone() });
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| VaultError::io(parent, e))?;
    }
    std::fs::rename(&note_file, &target).map_err(|e| VaultError::io(&note_file, e))?;
    let _ = std::fs::remove_file(&sidecar);
    Ok(entry.original)
}

/// Remove one entry from the vault trash, handing it to the operating system's
/// own trash rather than unlinking it.
pub fn purge(root: &Path, id: &str) -> VaultResult<()> {
    let (note_file, sidecar) = entry_paths(root, id);
    let mut victims = Vec::new();
    for path in [&note_file, &sidecar] {
        if path.exists() {
            victims.push(path_str(path)?.to_string());
        }
    }
    if victims.is_empty() {
        return Err(VaultError::NoteMissing { path: RelPath::new(id) });
    }
    arbor_fs::prelude::trash::trash(&victims).map_err(|e| VaultError::io(&note_file, e))
}

/// Empty the vault trash, one entry at a time so a single failure does not stop
/// the rest.
pub fn empty(root: &Path) -> VaultResult<Vec<String>> {
    let mut purged = Vec::new();
    for entry in list(root)? {
        purge(root, &entry.id)?;
        purged.push(entry.id);
    }
    Ok(purged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_carries_both_the_name_and_the_moment() {
        let path = RelPath::new("daily/2026-07-31.md");
        assert_eq!(entry_id(&path, "2026-07-31-1422"), "2026-07-31--2026-07-31-1422");
    }

    #[test]
    fn an_id_is_always_a_legal_file_name() {
        let path = RelPath::new("bugs/Crash: all'avvio?.md");
        assert_eq!(entry_id(&path, "2026-07-31-1422"), "Crash-all'avvio--2026-07-31-1422");
    }

    #[test]
    fn a_note_with_nothing_usable_in_its_name_still_gets_an_id() {
        assert_eq!(entry_id(&RelPath::new("bugs/  .md"), ""), "nota");
    }

    #[test]
    fn the_sidecar_round_trips() {
        let entry = TrashedNote {
            id: "Crash--2026-07-31-1422".into(),
            original: RelPath::new("bugs/Crash.md"),
            trashed_at: "2026-07-31-1422".into(),
            title: "Crash all'avvio".into(),
        };
        let text = toml::to_string_pretty(&entry).expect("the sidecar serialises");
        assert_eq!(toml::from_str::<TrashedNote>(&text).expect("and reads back"), entry);
    }
}
