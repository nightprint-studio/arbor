//! `repo` domain — leaf repository queries/metadata routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! The repo-lifecycle flow lives here (`open_repo` / `close_repo`). What stays
//! is path validation + the lifecycle: both mutate the open-repo set and mirror
//! into `corvus-be` (`sync_repo_open/close`) but emit through the backend event
//! sink (`state.emit`) and take no `AppHandle`. The pure `git`-identity /
//! metadata probes (`get_git_identity`, `get_repo_info`), the path / network
//! probes (`check_is_git_repo`, `clone_repo`, `list_remote_branches_for_url`)
//! and repository initialisation (`init_repo`) moved to `corvus-be`
//! (`crate::repo_ops` / `crate::repo_lifecycle` there) — that binary is their
//! sole owner now.
//!
//! The `on_repo_open` / `on_repo_close` (and the orphan-GC `on_repo_deregistered`)
//! hooks are fire-and-forget and fire inline after the repo lock is dropped,
//! with first-hand data.

use serde_json::json;

use crate::error::AppError;
use crate::git::repo::RepoInfo;
use crate::ipc::corvus;
use crate::AppState;

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
