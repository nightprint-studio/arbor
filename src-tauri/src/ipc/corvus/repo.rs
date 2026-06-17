//! `repo` domain — leaf repository queries/metadata routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! Only the genuinely leaf-clean ops live here: path/identity probes and
//! single-repo metadata reads. The coupled open/close/clone/init flows stay
//! inline in `repo_commands` for a later pass — they mutate the open-repo set,
//! call `sync_repo_*`, take an `AppHandle`, emit events, or run async (provider
//! REST + network clone), and they fire repo-lifecycle hooks.
//!
//! `check_is_git_repo` / `get_git_identity` / `list_remote_branches_for_url`
//! never touched `AppState`, but the handler macro requires a context first
//! arg, so they take `_state: &AppState` and ignore it. The original commands
//! returned bare `bool` / tuple values; the broker shape is `Result<R,
//! AppError>`, so the handlers wrap the same value in `Ok(...)` — the serde
//! shape on the wire is identical (a JSON bool / 2-tuple).
//!
//! No hooks fire in this domain.

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
