//! `repo` domain — leaf repository queries/metadata routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! The repo-lifecycle flow lives here (`open_repo` / `close_repo`). What stays is
//! the shell coordination: register/forget the tab with `corvus-be`
//! (`sync_repo_open/close`), fire the launcher's plugin hooks, run the orphan GC
//! and emit `arbor://registry-changed`. The git metadata itself (current branch,
//! bare/empty flags) comes from `corvus-be`'s `get_repo_info` — the launcher
//! opens no git2 handle and keeps no `RepoManager`. The pure `git`-identity /
//! metadata probes (`get_git_identity`, `get_repo_info`), the path / network
//! probes (`check_is_git_repo`, `clone_repo`, `list_remote_branches_for_url`)
//! and repository initialisation (`init_repo`) live in `corvus-be`
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

/// Open the repository at `path` under `tab_id`.
///
/// Registers the path with `corvus-be` (the open-tab registry owner) via
/// `sync_repo_open`, then asks it for the git metadata (`get_repo_info`) — the
/// launcher opens no git2 handle. Fires `on_repo_open` inline with that data.
#[corvus::handler]
fn open_repo(state: &AppState, path: String, tab_id: String) -> Result<RepoInfo, AppError> {
    // Register first so corvus-be can resolve `tab_id` → path for `get_repo_info`.
    crate::ipc::sync_repo_open(state, &tab_id, &path);
    let info: RepoInfo = serde_json::from_value(crate::ipc::dispatch_rpc(
        state,
        "corvus",
        "get_repo_info",
        json!({ "tab_id": tab_id }),
    )?)?;
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
    // Resolve the path from corvus-be BEFORE deregistering it. `name` is the
    // workdir basename — identical to the old `RepoInfo.name`.
    let path = crate::ipc::resolve_tab_path(state, &tab_id).unwrap_or_default();
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
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
