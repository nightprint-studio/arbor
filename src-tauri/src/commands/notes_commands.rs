use tauri::State;

use crate::error::Result;
use crate::git::notes::{CommitNote, NoteRemoteStatus};
use crate::AppState;

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Return all notes attached to a commit across every namespace.
/// `remote_status` is always `Unknown` here — use `check_note_remote_status`
/// when the modal opens to fill it in for a specific namespace.
#[tauri::command]
pub fn list_commit_notes(
    state: State<'_, AppState>,
    tab_id: String,
    commit_oid: String,
) -> Result<Vec<CommitNote>> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::notes::list_notes(repo.inner(), &commit_oid)
}

/// Check whether a note for `commit_oid` in `namespace` has been pushed to
/// the remote tracking ref `refs/remotes/origin/notes/<namespace>`.
/// Called lazily when the notes modal opens (not on graph load).
#[tauri::command]
pub fn check_note_remote_status(
    state: State<'_, AppState>,
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
#[tauri::command]
pub fn save_commit_note(
    state: State<'_, AppState>,
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
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":     &tab_id,
            "commit_oid": &commit_oid,
            "namespace":  &namespace,
        });
        let _ = host.fire_hook("on_note_saved", &ctx.to_string());
    }
    Ok(())
}

/// Push `refs/notes/<namespace>` to origin so others can fetch it.
#[tauri::command]
pub fn push_note_namespace(
    state: State<'_, AppState>,
    tab_id: String,
    namespace: String,
) -> Result<()> {
    let mut mgr  = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let refspec = format!("refs/notes/{ns}:refs/notes/{ns}", ns = namespace);
    crate::git::remote::push(repo.inner(), "origin", &refspec, false)
}

/// Delete the note for `commit_oid` in `refs/notes/<namespace>`.
#[tauri::command]
pub fn delete_commit_note(
    state: State<'_, AppState>,
    tab_id: String,
    commit_oid: String,
    namespace: String,
) -> Result<()> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::notes::delete_note(repo.inner(), &commit_oid, &namespace)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":     &tab_id,
            "commit_oid": &commit_oid,
            "namespace":  &namespace,
        });
        let _ = host.fire_hook("on_note_deleted", &ctx.to_string());
    }
    Ok(())
}
