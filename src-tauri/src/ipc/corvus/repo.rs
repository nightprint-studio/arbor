//! `repo` domain — leaf repository queries/metadata routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! The leaf-clean ops live here (path/identity probes and single-repo metadata
//! reads) alongside the `open_repo` / `close_repo` lifecycle flow. Both mutate
//! the open-repo set and mirror into `corvus-be` (`sync_repo_open/close`) but
//! emit through the backend event sink (`state.emit`) and take no `AppHandle`,
//! so they migrate cleanly. `clone_repo` lives here too — a pure clone-to-disk
//! that returns the fresh repo's metadata (no tab opened; the frontend opens
//! the tab afterwards via `open_repo`). `init_repo` stays inline in
//! `repo_commands` for the credential pass (it creates a remote via the git
//! provider + a host token, gated on the M3 credential broker).
//!
//! `check_is_git_repo` / `get_git_identity` / `list_remote_branches_for_url`
//! never touched `AppState`, but the handler macro requires a context first
//! arg, so they take `_state: &AppState` and ignore it. The original commands
//! returned bare `bool` / tuple values; the broker shape is `Result<R,
//! AppError>`, so the handlers wrap the same value in `Ok(...)` — the serde
//! shape on the wire is identical (a JSON bool / 2-tuple).
//!
//! The `on_repo_open` / `on_repo_close` (and the orphan-GC `on_repo_deregistered`)
//! hooks are fire-and-forget and fire inline after the repo lock is dropped,
//! with first-hand data.

use crate::error::AppError;
use crate::git::repo::{CloneOptions, RepoInfo};
use crate::ipc::corvus;
use crate::AppState;
use serde_json::json;

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

/// Clone a remote repository to disk and return the fresh repo's metadata.
///
/// Does **not** open a tab: the returned [`RepoInfo`] carries an empty `tab_id`,
/// and no `on_repo_open` hook fires. Opening the clone as a tab is the
/// frontend's job (via `open_repo`, keyed by the workspace-registry id) — every
/// caller already reopens under a canonical id, so registering a throwaway tab
/// here was pure waste. Runs the network clone on the broker's blocking thread
/// (the handler is sync), so the IPC/UI thread never stalls on it.
#[corvus::handler]
fn clone_repo(_state: &AppState, opts: CloneOptions) -> Result<RepoInfo, AppError> {
    let dest = crate::git::repo::clone_repo(&opts)?;
    RepoInfo::for_path(&dest)
}

/// Open the repository at `path` under `tab_id` in the repo manager.
///
/// Fires `on_repo_open` inline (after the repo lock is dropped) and calls
/// `sync_repo_open` to mirror the open into `corvus-be`.
#[corvus::handler]
fn open_repo(state: &AppState, path: String, tab_id: String) -> Result<RepoInfo, AppError> {
    let info = {
        let mut mgr = state.lock_repos()?;
        mgr.open(tab_id.clone(), &path)?
    };
    crate::ipc::sync_repo_open(state, &tab_id, &info.path);
    // Fire inline with first-hand data; the repo lock is already dropped above
    // so Lua git ops in the hook can't deadlock against our guard.
    state.fire_hook(
        "on_repo_open",
        json!({ "tab_id": tab_id, "path": info.path, "name": info.name }),
    );
    Ok(info)
}

/// Close the tab `tab_id` in the repo manager.
///
/// Fires `on_repo_close` inline (after the repo lock is dropped), mirrors the
/// close into `corvus-be` via `sync_repo_close`, then runs the orphan GC: a
/// repo with no open tab and no workspace membership is forgotten (registry
/// entry + recent-repos pointer dropped, `on_repo_deregistered` fired), and
/// `arbor://registry-changed` is emitted so the explorer's Projects view
/// refreshes.
#[corvus::handler]
fn close_repo(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let (path, name) = {
        let mut mgr = state.lock_repos()?;
        let info = mgr.get(&tab_id)
            .map(|r| (r.path.clone(), r.name.clone()))
            .unwrap_or_default();
        mgr.close(&tab_id);
        info
    };
    state.fire_hook(
        "on_repo_close",
        json!({ "tab_id": &tab_id, "path": &path, "name": &name }),
    );
    crate::ipc::sync_repo_close(state, &tab_id);

    // The shared GC helper re-checks both orphan conditions (no open tab AND no
    // workspace membership) itself before dropping the registry entry.
    if !path.is_empty() {
        let repo_id = state.lock_repo_registry()
            .ok()
            .and_then(|reg| reg.find_by_path(&path).map(|e| e.id.clone()));
        if let Some(id) = repo_id {
            let forgotten = crate::commands::workspace_commands::forget_repo_if_orphaned(
                state, &id, "tab_closed_when_orphan",
            ).unwrap_or(false);
            // Dropping the registry entry changes the explorer's Projects view.
            // `()` serializes to JSON null — byte-identical to the old `app.emit`.
            if forgotten {
                state.emit("arbor://registry-changed", ());
            }
        }
    }
    Ok(())
}
