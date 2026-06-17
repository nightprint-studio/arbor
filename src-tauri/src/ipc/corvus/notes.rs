//! `notes` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. Behavior
//! (locks held, hooks fired, errors) is byte-identical — only the call path
//! changed.

use crate::error::Result;
use crate::git::notes::{CommitNote, NoteRemoteStatus};
use crate::ipc::corvus;
use crate::AppState;

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Return all notes attached to a commit across every namespace.
/// `remote_status` is always `Unknown` here — use `check_note_remote_status`
/// when the modal opens to fill it in for a specific namespace.
#[corvus::handler]
fn list_commit_notes(state: &AppState, tab_id: String, commit_oid: String) -> Result<Vec<CommitNote>> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::notes::list_notes(repo.inner(), &commit_oid)
}

/// Check whether a note for `commit_oid` in `namespace` has been pushed to
/// the remote tracking ref `refs/remotes/origin/notes/<namespace>`.
/// Called lazily when the notes modal opens (not on graph load).
#[corvus::handler]
fn check_note_remote_status(
    state: &AppState,
    tab_id: String,
    commit_oid: String,
    namespace: String,
) -> Result<NoteRemoteStatus> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::notes::check_remote_status(repo.inner(), &commit_oid, &namespace)
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Create or update a note for `commit_oid` in `refs/notes/<namespace>`.
#[corvus::handler]
fn save_commit_note(
    state: &AppState,
    tab_id: String,
    commit_oid: String,
    namespace: String,
    content: String,
) -> Result<()> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::notes::set_note(repo.inner(), &commit_oid, &namespace, &content)?;
    }
    state.fire_hook(
        "on_note_saved",
        serde_json::json!({
            "tab_id":     &tab_id,
            "commit_oid": &commit_oid,
            "namespace":  &namespace,
        }),
    );
    Ok(())
}

/// Push `refs/notes/<namespace>` to origin so others can fetch it.
#[corvus::handler]
fn push_note_namespace(state: &AppState, tab_id: String, namespace: String) -> Result<()> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let refspec = format!("refs/notes/{ns}:refs/notes/{ns}", ns = namespace);
    crate::git::remote::push(repo.inner(), "origin", &refspec, false)
}

/// Delete the note for `commit_oid` in `refs/notes/<namespace>`.
#[corvus::handler]
fn delete_commit_note(
    state: &AppState,
    tab_id: String,
    commit_oid: String,
    namespace: String,
) -> Result<()> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::notes::delete_note(repo.inner(), &commit_oid, &namespace)?;
    }
    state.fire_hook(
        "on_note_deleted",
        serde_json::json!({
            "tab_id":     &tab_id,
            "commit_oid": &commit_oid,
            "namespace":  &namespace,
        }),
    );
    Ok(())
}
