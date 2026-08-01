//! `trash` domain — the vault's own undo for a deleted note.
//!
//! Every handler here is three lines, and that is the point: `garrulus-vault`'s
//! `trash` module already owns the whole model — the sidecar that records where a
//! note came from, the id collision handling, and the fact that a purge hands the
//! file to the *operating system's* trash rather than unlinking it. Re-deriving
//! any of that here would be a second implementation of a thing that must have
//! exactly one.
//!
//! What this file adds is the two things the vault crate cannot know about: the
//! open vault's root, and the fact that a restored note has to go back into the
//! index and be announced to plugins.

use garrulus_core::prelude::{hooks, trash, GarrulusState, TrashedNote};
use serde_json::json;

use crate::note;

/// Everything currently in the vault's trash, oldest id first.
///
/// A sidecar whose note has gone missing is skipped by the vault crate rather
/// than reported: it is debris from an interrupted delete and there is nothing
/// the user could do about it.
#[arbor_rpc::handler]
fn garrulus_trash_list(state: &GarrulusState) -> Result<Vec<TrashedNote>, String> {
    let root = state.vault_root()?;
    trash::list(&root).map_err(|e| e.to_string())
}

/// Put a trashed note back where it came from, and back into the index.
///
/// Refuses when something is already at the original path — restoring over a note
/// written since the delete would lose the newer one, and that is the vault
/// crate's call, not this handler's.
#[arbor_rpc::handler]
fn garrulus_trash_restore(state: &GarrulusState, id: String) -> Result<(), String> {
    let root = state.vault_root()?;
    let restored = trash::restore(&root, &id).map_err(|e| e.to_string())?;
    let path = restored.as_str().to_string();

    // The index is a cache and the note is back on disk; without this it would
    // stay invisible to search and to backlinks until the next vault open.
    note::reindex(state, &path)?;
    // Fired with every guard dropped — `reindex` takes and releases its own.
    // `note_created` rather than a trash-specific hook: from a plugin's point of
    // view a note that was not in the vault now is, which is the same event.
    state.fire_hook(hooks::NOTE_CREATED, json!({ "path": path, "source": "trash" }));
    Ok(())
}

/// Remove one entry from the vault's trash.
///
/// Still not a hard delete: the files go to the OS trash, so even this deliberate,
/// confirmed second delete is recoverable outside Arbor.
#[arbor_rpc::handler]
fn garrulus_trash_purge(state: &GarrulusState, id: String) -> Result<(), String> {
    let root = state.vault_root()?;
    trash::purge(&root, &id).map_err(|e| e.to_string())
}

/// Empty the vault's trash.
///
/// The vault crate purges one entry at a time so a single failure does not strand
/// the rest; the ids it managed to remove are of no use to the caller, which is
/// about to reload the list anyway.
#[arbor_rpc::handler]
fn garrulus_trash_empty(state: &GarrulusState) -> Result<(), String> {
    let root = state.vault_root()?;
    trash::empty(&root).map(|_| ()).map_err(|e| e.to_string())
}
