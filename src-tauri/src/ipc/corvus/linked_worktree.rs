//! `linked_worktree` domain — read-only registry queries routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. Only the
//! **leaf-clean queries** live here: they take `&AppState`, hold the
//! linked-worktree registry lock just long enough to read, and fire no hooks.
//!
//! NOT migrated (stay inline in `linked_worktree_commands`, handled by a later
//! emit/seam pass): every mutation (`create`/`delete`/`rename`/member add+remove/
//! sync-enable/alias-group CRUD). They all take an `AppHandle` and emit
//! `arbor://worktree-links-changed`; `add_worktree_link_member` /
//! `remove_worktree_link_member` additionally fire the
//! `on_worktree_link_member_added` / `_removed` hooks. Those hooks must move to
//! `post_hooks.rs` when the mutations migrate.
//!
//! No hooks fire in this (read-only) domain.

use crate::error::AppError;
use crate::ipc::corvus;
use crate::linked_worktrees::WorktreeLink;
use crate::AppState;

#[corvus::handler]
fn list_worktree_links(state: &AppState) -> Result<Vec<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.list())
}

#[corvus::handler]
fn get_worktree_link(state: &AppState, id: String) -> Result<Option<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.get(&id).cloned())
}

#[corvus::handler]
fn get_worktree_link_for_repo(
    state: &AppState,
    repo_id: String,
) -> Result<Option<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.find_by_repo(&repo_id).cloned())
}
