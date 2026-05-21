use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::git::branch::{BranchInfo, RemoteRenameResult, TagInfo};
use crate::git::stash::StashEntry;
use crate::git::status::RepoStatus;
use crate::AppState;
use crate::linked_worktrees;

/// Look up the RepoRegistry UUID for a given tab.  Returns None if the tab
/// is not registered (defensive — every opened tab should be).
fn repo_id_for_tab(state: &AppState, tab_id: &str) -> Option<String> {
    let path = {
        let mut mgr = state.lock_repos().ok()?;
        let info = mgr.get(tab_id).ok()?;
        info.path.clone()
    };
    let reg = state.lock_repo_registry().ok()?;
    let result = reg.find_by_path(&path).map(|e| e.id.clone());
    if result.is_none() {
        tracing::warn!(
            "repo_id_for_tab: tab_id={} path={:?} not in registry — link sync will not trigger",
            tab_id, path
        );
    }
    result
}

/// Returned by `checkout_branch_safe` so the frontend knows whether a
/// pre-checkout stash needs to be re-applied and whether that re-apply had conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutResult {
    pub stash_conflicts: Vec<String>,
    pub pre_checkout_stash: Option<StashEntry>,
    /// Non-None when stash re-apply failed for a non-conflict reason (e.g. file lock).
    /// The stash entry is preserved at index 0 — user must apply it manually.
    pub stash_apply_error: Option<String>,
}

#[tauri::command]
pub fn get_status(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<RepoStatus, AppError> {
    // Read the detect_renames flag from user config BEFORE taking the repos
    // lock, so we don't nest the two mutexes.
    let detect_renames = state
        .lock_config()
        .map(|c| c.status.detect_renames)
        .unwrap_or(true);
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::status::get_status_with(repo.inner(), detect_renames)
}

#[tauri::command]
pub fn list_local_branches(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_local_branches(repo.inner())
}

#[tauri::command]
pub fn list_remote_branches(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_remote_branches(repo.inner())
}

#[tauri::command]
pub fn list_tags(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<TagInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_tags(repo.inner())
}

#[tauri::command]
pub fn get_nearest_tag(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(crate::git::branch::get_nearest_tag(repo.inner()))
}

#[tauri::command]
pub fn create_branch(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
    from_oid: String,
) -> Result<BranchInfo, AppError> {
    // Refuse names that would conflict with an active alias mapping in any
    // space this repo belongs to.  The user must remove the alias first.
    if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
        if let Ok(reg) = state.lock_linked_worktrees() {
            let all = reg.list();
            if let Some(link_name) = linked_worktrees::aliases::alias_blocks_branch_name(&all, &repo_id, &name) {
                return Err(AppError::Other(format!(
                    "branch '{name}' is reserved by an alias in worktree link '{link_name}' — remove the alias to free this name"
                )));
            }
        }
    }
    let info = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::create_branch(repo.inner(), &name, &from_oid)?
    };
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "name": &name, "from_oid": &from_oid });
        let _ = host.fire_hook("on_branch_create", &ctx.to_string());
    }
    Ok(info)
}

#[tauri::command]
pub fn delete_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::delete_branch(repo.inner(), &name)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "name": &name });
        let _ = host.fire_hook("on_branch_delete", &ctx.to_string());
    }
    // Remove alias entries that referenced this (repo_id, branch).
    if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
        if let Ok(mut reg) = state.lock_linked_worktrees() {
            let mut all = reg.list();
            let removed = linked_worktrees::aliases::on_branch_deleted(&mut all, &repo_id, &name);
            if removed > 0 {
                reg.replace_all(all);
                let _ = linked_worktrees::save(&reg);
                use tauri::Emitter;
                let _ = app.emit("arbor://worktree-links-changed", serde_json::json!({}));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn rename_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    old_name: String,
    new_name: String,
) -> Result<BranchInfo, AppError> {
    let info = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::rename_branch(repo.inner(), &old_name, &new_name)?
    };
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "old_name": &old_name, "new_name": &new_name });
        let _ = host.fire_hook("on_branch_rename", &ctx.to_string());
    }
    // Smart-update alias entries: rename in-place; collapse groups that
    // become trivial (every member shares the same branch name).
    if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
        if let Ok(mut reg) = state.lock_linked_worktrees() {
            let mut all = reg.list();
            let affected = linked_worktrees::aliases::on_branch_renamed(&mut all, &repo_id, &old_name, &new_name);
            if affected > 0 {
                reg.replace_all(all);
                let _ = linked_worktrees::save(&reg);
                use tauri::Emitter;
                let _ = app.emit("arbor://worktree-links-changed", serde_json::json!({}));
            }
        }
    }
    Ok(info)
}

#[tauri::command]
pub fn checkout_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::checkout_branch(repo.inner(), &name)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "branch": &name });
        let _ = host.fire_hook("on_checkout", &ctx.to_string());
    }
    // Trigger space sync if this tab's repo is a member of a sync-enabled
    // space.  No-op otherwise.  Runs in a background thread.
    if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
        linked_worktrees::orchestrator::maybe_trigger_checkout_sync(&app, &tab_id, &repo_id, &name);
    }
    Ok(())
}

/// Stash-safe checkout: stash dirty workdir → checkout branch → stash apply.
/// Uses stash_apply (never pop) so the stash is preserved on conflict or error.
#[tauri::command]
pub fn checkout_branch_safe(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<CheckoutResult, AppError> {
    // Step 1: get workdir + dirty check (immutable borrow, then drop lock).
    let (workdir, is_dirty) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let r = repo.inner();
        let workdir = r
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository".into()))?
            .to_path_buf();
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        let statuses = r.statuses(Some(&mut opts))?;
        let dirty = statuses.iter().any(|s| s.status() != git2::Status::CURRENT);
        (workdir, dirty)
    };

    // Step 2: CLI stash — does not hold the Rust mutex.
    let stash_entry: Option<StashEntry> = if is_dirty {
        let entry =
            crate::git::stash::stash_save(&workdir, Some("arbor: pre-checkout stash"), true)
                .map_err(|e| AppError::Other(format!("stash failed: {e}")))?;
        Some(entry)
    } else {
        None
    };

    // Step 3: checkout + stash re-apply (mutable borrow).
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        let r = repo.inner_mut();

        crate::git::recovery::try_snapshot(
            r,
            crate::git::recovery::RecoveryKind::Checkout,
            format!("checkout branch '{name}' (safe)"),
        );

        // Catch checkout failures explicitly when there is a pre-checkout
        // stash to surface — propagating via `?` would lose the stash
        // context and the frontend would only see a generic "Checkout
        // failed" toast, leaving the user blind to the preserved stash
        // (and any partial workdir state libgit2 may have left behind on
        // Windows when the snapshot mechanism couldn't roll it back).
        if let Err(e) = crate::git::branch::checkout_branch(r, &name) {
            return Ok(CheckoutResult {
                stash_conflicts: vec![],
                pre_checkout_stash: stash_entry.as_ref().map(|s| StashEntry {
                    index: 0,
                    message: s.message.clone(),
                    oid: s.oid.clone(),
                }),
                stash_apply_error: Some(format!("checkout failed: {e}")),
            });
        }

        if let Some(ref saved) = stash_entry {
            match crate::git::stash::stash_apply(r, 0) {
                Ok(res) if res.has_conflicts => {
                    let entry = StashEntry {
                        index: 0,
                        message: saved.message.clone(),
                        oid: saved.oid.clone(),
                    };
                    CheckoutResult {
                        stash_conflicts: res.conflicted_files,
                        pre_checkout_stash: Some(entry),
                        stash_apply_error: None,
                    }
                }
                Ok(_) => {
                    // Clean apply — drop the preserved stash entry.
                    let _ = r.stash_drop(0);
                    CheckoutResult {
                        stash_conflicts: vec![],
                        pre_checkout_stash: None,
                        stash_apply_error: None,
                    }
                }
                Err(e) => {
                    let entry = StashEntry {
                        index: 0,
                        message: saved.message.clone(),
                        oid: saved.oid.clone(),
                    };
                    CheckoutResult {
                        stash_conflicts: vec![],
                        pre_checkout_stash: Some(entry),
                        stash_apply_error: Some(e.to_string()),
                    }
                }
            }
        } else {
            CheckoutResult {
                stash_conflicts: vec![],
                pre_checkout_stash: None,
                stash_apply_error: None,
            }
        }
    };

    // Fire hook only on clean success.
    let clean = result.stash_apply_error.is_none() && result.stash_conflicts.is_empty();
    if clean {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "branch": &name });
            let _ = host.fire_hook("on_checkout", &ctx.to_string());
        }
        // Trigger space sync only after a clean checkout.
        if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
            linked_worktrees::orchestrator::maybe_trigger_checkout_sync(&app, &tab_id, &repo_id, &name);
        }
    }

    Ok(result)
}

/// Create a local tracking branch for `remote_name` (e.g. "origin/patch/4.14")
/// if missing, then checkout the resulting local branch. Returns the local
/// short name so the frontend can update its tab badge / conflict modal.
#[tauri::command]
pub fn checkout_remote_as_local(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    remote_name: String,
) -> Result<String, AppError> {
    let local_name = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::checkout_remote_as_local(repo.inner(), &remote_name)?
    };
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "branch": &local_name });
        let _ = host.fire_hook("on_checkout", &ctx.to_string());
    }
    if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
        linked_worktrees::orchestrator::maybe_trigger_checkout_sync(&app, &tab_id, &repo_id, &local_name);
    }
    Ok(local_name)
}

#[tauri::command]
pub fn checkout_commit(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::checkout_commit_detached(repo.inner(), &oid)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "oid": &oid });
        let _ = host.fire_hook("on_checkout", &ctx.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn list_merged_branches(
    state: State<'_, AppState>,
    tab_id: String,
    target: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_merged_branches(repo.inner(), &target)
}

#[tauri::command]
pub fn list_merged_remote_branches(
    state: State<'_, AppState>,
    tab_id: String,
    target: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_merged_remote_branches(repo.inner(), &target)
}

#[tauri::command]
pub fn delete_remote_branches(
    state: State<'_, AppState>,
    tab_id: String,
    names: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let deleted_names: Vec<String> = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let failed = crate::git::branch::delete_remote_branches(repo.inner(), &names);
        names.iter().filter(|n| !failed.contains(n)).cloned().collect()
    };
    if !deleted_names.is_empty() {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "names": &deleted_names });
            let _ = host.fire_hook("on_branch_delete", &ctx.to_string());
        }
    }
    // Return the failed names (same convention as delete_branches)
    let failed: Vec<String> = names.into_iter().filter(|n| !deleted_names.contains(n)).collect();
    Ok(failed)
}

#[tauri::command]
pub fn rename_remote_branch(
    state: State<'_, AppState>,
    tab_id: String,
    old_full_name: String,
    new_short_name: String,
    rename_local: bool,
) -> Result<RemoteRenameResult, AppError> {
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::rename_remote_branch(
            repo.inner(),
            &old_full_name,
            &new_short_name,
            rename_local,
        )?
    };
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id": &tab_id,
            "old_name": &old_full_name,
            "new_name": &result.new_full_name,
            "local_renamed": result.local_renamed,
        });
        let _ = host.fire_hook("on_branch_rename", &ctx.to_string());
    }
    Ok(result)
}

#[tauri::command]
pub fn delete_branches(
    state: State<'_, AppState>,
    tab_id: String,
    names: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let deleted = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::delete_branches(repo.inner(), &names)
    };
    if !deleted.is_empty() {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "names": &deleted });
            let _ = host.fire_hook("on_branch_delete", &ctx.to_string());
        }
    }
    Ok(deleted)
}
