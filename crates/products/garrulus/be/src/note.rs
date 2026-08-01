//! `note` domain — read / write / create / rename / delete one note.
//!
//! The bytes are the record. Every handler here writes what it was given and then
//! *re-reads* the note to refresh the index, rather than predicting what the
//! parse will say — the index is a cache, and a cache that guesses is a cache that
//! drifts.
//!
//! Deletion moves the file into `<vault>/.arbor/garrulus/trash/`. Notes are not
//! build artefacts: deleting one has to be undoable without going through git.

use garrulus_core::prelude::{civil_from_unix, hooks, trash_note, GarrulusState, Note, RelPath};
use serde::Serialize;
use serde_json::json;

use crate::vault_io;

/// A note's source, as it is on disk.
#[derive(Debug, Clone, Serialize)]
pub struct NoteSource {
    /// Vault-relative path, POSIX separators.
    pub path: String,
    /// The file's text, verbatim — no reformatting, ever (the frontmatter
    /// round-trip invariant depends on it).
    pub text: String,
}

/// Read a note's source text.
#[arbor_rpc::handler]
fn garrulus_read_note(state: &GarrulusState, path: String) -> Result<NoteSource, String> {
    let root = state.vault_root()?;
    let text = vault_io::read_source(&root, &path)?;
    Ok(NoteSource { path, text })
}

/// Write a note's source text and refresh its index entry.
#[arbor_rpc::handler]
fn garrulus_write_note(state: &GarrulusState, path: String, text: String) -> Result<(), String> {
    let root = state.vault_root()?;
    vault_io::write_source(&root, &path, &text)?;
    reindex(state, &path)?;
    // Every guard is dropped by here — Lua in this hook may call back in.
    // `garrulus:note_saved` — the namespace, not an infix in the event name, is
    // what keeps this apart from corvus's git-note hook of the same two words
    // (which carries `{tab_id, commit_oid, namespace}` instead).
    state.fire_hook(hooks::NOTE_SAVED, json!({ "path": path, "bytes": text.len() }));
    Ok(())
}

/// Create a note that does not exist yet, optionally with a body (a rendered
/// template). Refuses to overwrite: creating over an existing note is how a
/// vault loses text.
#[arbor_rpc::handler]
fn garrulus_create_note(
    state: &GarrulusState,
    path: String,
    text: Option<String>,
) -> Result<NoteSource, String> {
    let root = state.vault_root()?;
    if vault_io::exists(&root, &path)? {
        return Err(format!("{path}: a note is already there"));
    }
    let text = text.unwrap_or_default();
    vault_io::write_source(&root, &path, &text)?;
    reindex(state, &path)?;
    state.fire_hook(hooks::NOTE_CREATED, json!({ "path": path }));
    Ok(NoteSource { path, text })
}

/// Move a note to a new vault-relative path.
///
/// Note that this does **not** rewrite the `[[wikilinks]]` that pointed at it —
/// rename-with-link-update is its own flow (it needs a preview of what changes,
/// per `docs/garrulus-design.md` §7.3) and will call this once the rewrite is
/// agreed. The index is refreshed here, so the links that broke show up in
/// `garrulus_problems` immediately either way.
#[arbor_rpc::handler]
fn garrulus_rename_note(
    state: &GarrulusState,
    path: String,
    new_path: String,
) -> Result<(), String> {
    let root = state.vault_root()?;
    if vault_io::exists(&root, &new_path)? {
        return Err(format!("{new_path}: a note is already there"));
    }
    let from = vault_io::resolve_rel(&root, &path)?;
    let to = vault_io::resolve_rel(&root, &new_path)?;
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{new_path}: {e}"))?;
    }
    // Read the note BEFORE the move: its index entry is keyed by the note's own
    // id, which can only be learned from the file while the file is still there.
    let previous = current_note(state, &path);
    std::fs::rename(&from, &to).map_err(|e| format!("{path} → {new_path}: {e}"))?;

    deindex(state, previous);
    reindex(state, &new_path)?;
    state.fire_hook(hooks::NOTE_RENAMED, json!({ "old_path": path, "new_path": new_path }));
    Ok(())
}

/// Delete a note by moving it into the vault's trash, timestamped so two deletes
/// of the same name never collide.
#[arbor_rpc::handler]
fn garrulus_delete_note(state: &GarrulusState, path: String) -> Result<(), String> {
    let root = state.vault_root()?;
    // Guard the path first — `trash_note` refuses an escaping one too, but the
    // error a handler should return is this crate's, not the vault's.
    vault_io::resolve_rel(&root, &path)?;
    let rel = RelPath::new(&path);

    // As in the rename: learn the note's id and title while the file still exists.
    let previous = current_note(state, &path);
    let title = previous
        .as_ref()
        .map(|n| n.title.clone())
        .unwrap_or_else(|| rel.stem().to_string());

    // `trash_note`, never a hand-rolled move: it writes the `.toml` sidecar that
    // records where the note came from, and without that sidecar nothing deleted
    // through this backend could ever be restored.
    let (date, time) = civil_from_unix(vault_io::now_ms() / 1000);
    let stamp = format!("{date}-{}", time.replace(':', ""));
    let entry = trash_note(&root, &rel, &title, &stamp).map_err(|e| e.to_string())?;

    deindex(state, previous);
    state.fire_hook(
        hooks::NOTE_DELETED,
        json!({ "path": path, "trash_id": entry.id }),
    );
    Ok(())
}

/// Re-read a note and upsert it into the index.
///
/// Two statements, two lock scopes: the vault's read guard is dropped by the time
/// the index's write guard is taken (see `garrulus_core::state`'s lock order).
/// `pub(crate)` because every handler that rewrites a note owes the index the same
/// refresh — `apply_type` included — and there must be one implementation of it.
pub(crate) fn reindex(state: &GarrulusState, rel: &str) -> Result<(), String> {
    let note = vault_io::with_vault(state, |v| vault_io::load_note(v, rel))?;
    state.index_write()?.upsert(note);
    Ok(())
}

/// Read a note as it currently stands, for the sake of its id.
///
/// `None` when it cannot be read or parsed. That is not swallowing an error: the
/// caller is about to move or delete the file, and a note that never parsed has no
/// index entry to remove in the first place.
fn current_note(state: &GarrulusState, rel: &str) -> Option<Note> {
    vault_io::with_vault(state, |v| vault_io::load_note(v, rel)).ok()
}

/// Drop a note from the index by the id read from it before it moved.
///
/// Silent on a poisoned index or an unreadable note: the entry is then stale
/// rather than wrong, the next vault open clears it, and refusing to complete a
/// delete because a cache could not be tidied would be the worse failure.
fn deindex(state: &GarrulusState, note: Option<Note>) {
    let Some(note) = note else { return };
    if let Ok(mut index) = state.index_write() {
        index.remove(&note.id);
    }
}
