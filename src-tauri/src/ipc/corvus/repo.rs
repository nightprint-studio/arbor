//! `repo` domain — leaf repository queries/metadata routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! The leaf-clean ops live here (path/identity probes and single-repo metadata
//! reads) alongside the `open_repo` lifecycle flow. `open_repo` mutates the
//! open-repo set and calls `sync_repo_open`, but emits nothing of its own and
//! takes no `AppHandle`, so it migrates cleanly. The async provider/network
//! flows (`init_repo`, `clone_repo`) and `close_repo` stay inline in
//! `repo_commands` for a later pass — `close_repo`'s orphan-GC step calls a
//! `workspace_commands` helper still typed against `&State<'_, AppState>`.
//!
//! `check_is_git_repo` / `get_git_identity` / `list_remote_branches_for_url`
//! never touched `AppState`, but the handler macro requires a context first
//! arg, so they take `_state: &AppState` and ignore it. The original commands
//! returned bare `bool` / tuple values; the broker shape is `Result<R,
//! AppError>`, so the handlers wrap the same value in `Ok(...)` — the serde
//! shape on the wire is identical (a JSON bool / 2-tuple).
//!
//! The `on_repo_open` hook is fire-and-forget and fires from the generic `rpc`
//! post-hooks path (see `post_hooks.rs`), not inline here — the migrated
//! handler fires no hook itself.

use crate::error::AppError;
use crate::git::repo::RepoInfo;
use crate::ipc::corvus;
use crate::AppState;

/// Returns true when `path` is inside a git repository.
#[corvus::handler]
fn check_is_git_repo(_state: &AppState, path: String) -> Result<bool, AppError> {
    Ok(crate::git::init::is_git_repo(&path))
}

/// Read user.name / user.email from the global git config.
/// Returns ("", "") when the config is unavailable.
#[corvus::handler]
fn get_git_identity(_state: &AppState) -> Result<(String, String), AppError> {
    Ok(crate::git::init::get_git_identity())
}

/// List branch names available on a remote URL (calls `git ls-remote --heads`).
#[corvus::handler]
fn list_remote_branches_for_url(_state: &AppState, url: String) -> Result<Vec<String>, AppError> {
    crate::git::repo::list_remote_branches(&url)
}

#[corvus::handler]
fn get_repo_info(state: &AppState, tab_id: String) -> Result<RepoInfo, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(RepoInfo {
        tab_id: tab_id.clone(),
        path: repo.path.clone(),
        name: repo.name.clone(),
        current_branch: repo.current_branch(),
        is_bare: repo.inner().is_bare(),
        is_empty: repo.inner().is_empty().unwrap_or(false),
    })
}

/// Open the repository at `path` under `tab_id` in the repo manager.
///
/// Fires `on_repo_open` from the post-hooks path (not here). Calls
/// `sync_repo_open` to mirror the open into `corvus-be`.
#[corvus::handler]
fn open_repo(state: &AppState, path: String, tab_id: String) -> Result<RepoInfo, AppError> {
    let info = {
        let mut mgr = state.lock_repos()?;
        mgr.open(tab_id.clone(), &path)?
    };
    crate::ipc::sync_repo_open(state, &tab_id, &info.path);
    Ok(info)
}
