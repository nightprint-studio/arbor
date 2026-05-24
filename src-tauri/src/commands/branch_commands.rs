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

/// Returned by every `checkout_*_safe` command so the frontend knows whether a
/// pre-checkout stash needs to be re-applied and whether that re-apply had conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutResult {
    pub stash_conflicts: Vec<String>,
    pub pre_checkout_stash: Option<StashEntry>,
    /// Non-None when stash re-apply failed for a non-conflict reason (e.g. file lock).
    /// The stash entry is preserved at index 0 — user must apply it manually.
    pub stash_apply_error: Option<String>,
    /// Populated by `checkout_remote_as_local_safe` — the short name of the local
    /// branch created (or reused) for the remote ref. `None` for branch/commit
    /// checkouts. Lets the frontend update the tab badge and any modal that
    /// references the resolved name without a second IPC.
    #[serde(default)]
    pub resolved_local_name: Option<String>,
    /// True when the backend created a pre-checkout stash. Survives the clean-apply
    /// path (where `pre_checkout_stash` is set to None after `stash_drop`), so the
    /// frontend can surface "local changes stashed and restored" in the success
    /// toast even on a fully clean round-trip.
    #[serde(default)]
    pub did_stash: bool,
}

/// Stash-safe checkout core: stash dirty workdir → run `do_checkout` → apply stash.
///
/// `op_desc` is used in the recovery snapshot label. `do_checkout` performs the
/// actual ref/HEAD move on a `&mut Repository` and optionally returns a
/// resolved local branch name (used by `checkout_remote_as_local_safe`).
///
/// Mirrors the pull auto-stash dance (stash → mutate → apply, never pop, so
/// the stash survives conflicts and apply errors).
fn safe_checkout_with_stash<F>(
    state: &AppState,
    tab_id: &str,
    op_desc: &str,
    do_checkout: F,
) -> Result<CheckoutResult, AppError>
where
    F: FnOnce(&mut git2::Repository) -> Result<Option<String>, AppError>,
{
    // Step 1: workdir + dirty check (immutable borrow, then drop lock).
    let (workdir, is_dirty) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(tab_id)?;
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
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(tab_id)?;
    let r = repo.inner_mut();

    crate::git::recovery::try_snapshot(
        r,
        crate::git::recovery::RecoveryKind::Checkout,
        op_desc.to_string(),
    );

    let did_stash = stash_entry.is_some();

    // Catch checkout failures explicitly when there is a pre-checkout stash to
    // surface — propagating via `?` would lose the stash context and the
    // frontend would only see a generic "Checkout failed" toast, leaving the
    // user blind to the preserved stash.
    let resolved_local_name = match do_checkout(r) {
        Ok(name) => name,
        Err(e) => {
            return Ok(CheckoutResult {
                stash_conflicts: vec![],
                pre_checkout_stash: stash_entry.as_ref().map(|s| StashEntry {
                    index: 0,
                    message: s.message.clone(),
                    oid: s.oid.clone(),
                }),
                stash_apply_error: Some(format!("checkout failed: {e}")),
                resolved_local_name: None,
                did_stash,
            });
        }
    };

    let mut result = if let Some(ref saved) = stash_entry {
        match crate::git::stash::stash_apply(r, 0) {
            Ok(res) if res.has_conflicts => CheckoutResult {
                stash_conflicts: res.conflicted_files,
                pre_checkout_stash: Some(StashEntry {
                    index: 0,
                    message: saved.message.clone(),
                    oid: saved.oid.clone(),
                }),
                stash_apply_error: None,
                resolved_local_name: None,
                did_stash,
            },
            Ok(_) => {
                // Clean apply — drop the preserved stash entry.
                let _ = r.stash_drop(0);
                CheckoutResult {
                    stash_conflicts: vec![],
                    pre_checkout_stash: None,
                    stash_apply_error: None,
                    resolved_local_name: None,
                    did_stash,
                }
            }
            Err(e) => CheckoutResult {
                stash_conflicts: vec![],
                pre_checkout_stash: Some(StashEntry {
                    index: 0,
                    message: saved.message.clone(),
                    oid: saved.oid.clone(),
                }),
                stash_apply_error: Some(e.to_string()),
                resolved_local_name: None,
                did_stash,
            },
        }
    } else {
        CheckoutResult {
            stash_conflicts: vec![],
            pre_checkout_stash: None,
            stash_apply_error: None,
            resolved_local_name: None,
            did_stash,
        }
    };

    result.resolved_local_name = resolved_local_name;
    Ok(result)
}

/// True when a CheckoutResult represents a fully clean operation (no stash
/// re-apply error, no stash conflicts). Used to gate hook firing and side
/// effects like worktree-link sync.
fn checkout_is_clean(r: &CheckoutResult) -> bool {
    r.stash_apply_error.is_none() && r.stash_conflicts.is_empty()
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

/// Stash-safe branch checkout: stash dirty workdir → checkout branch → stash apply.
/// Uses stash_apply (never pop) so the stash is preserved on conflict or error.
#[tauri::command]
pub fn checkout_branch_safe(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<CheckoutResult, AppError> {
    let name_for_checkout = name.clone();
    let result = safe_checkout_with_stash(
        &state,
        &tab_id,
        &format!("checkout branch '{name}' (safe)"),
        |r| {
            crate::git::branch::checkout_branch(r, &name_for_checkout)?;
            Ok(None)
        },
    )?;

    // Fire hook + trigger worktree-link sync only on clean success.
    if checkout_is_clean(&result) {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "branch": &name });
            let _ = host.fire_hook("on_checkout", &ctx.to_string());
        }
        if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
            linked_worktrees::orchestrator::maybe_trigger_checkout_sync(&app, &tab_id, &repo_id, &name);
        }
    }

    Ok(result)
}

/// Create a local tracking branch for `remote_name` (e.g. "origin/patch/4.14")
/// if missing, then checkout the resulting local branch. Returns the local
/// short name so the frontend can update its tab badge / conflict modal.
///
/// Non-safe variant — kept for backward compat and for the rare caller that
/// wants a hard failure on dirty workdir. New callers should use
/// `checkout_remote_as_local_safe`.
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

/// Stash-safe variant of `checkout_remote_as_local`: stash dirty workdir →
/// create local tracking branch (if missing) → switch HEAD → re-apply stash.
/// The resolved local branch short name is returned in `resolved_local_name`
/// even when stash apply produces conflicts, so the tab badge can be updated.
#[tauri::command]
pub fn checkout_remote_as_local_safe(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    remote_name: String,
) -> Result<CheckoutResult, AppError> {
    let remote_for_checkout = remote_name.clone();
    let result = safe_checkout_with_stash(
        &state,
        &tab_id,
        &format!("checkout remote '{remote_name}' as local (safe)"),
        |r| {
            let local = crate::git::branch::checkout_remote_as_local(r, &remote_for_checkout)?;
            Ok(Some(local))
        },
    )?;

    if checkout_is_clean(&result) {
        if let Some(ref local_name) = result.resolved_local_name {
            if let Ok(host) = state.lock_plugin_host() {
                let ctx = serde_json::json!({ "tab_id": &tab_id, "branch": local_name });
                let _ = host.fire_hook("on_checkout", &ctx.to_string());
            }
            if let Some(repo_id) = repo_id_for_tab(&state, &tab_id) {
                linked_worktrees::orchestrator::maybe_trigger_checkout_sync(&app, &tab_id, &repo_id, local_name);
            }
        }
    }

    Ok(result)
}

/// Non-safe commit checkout — kept for backward compat. Errors out on dirty
/// workdir (libgit2 Conflict). New callers should use `checkout_commit_safe`.
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

/// Stash-safe detached commit checkout: stash dirty workdir → detach HEAD →
/// re-apply stash. Mirrors `checkout_branch_safe` for the detached-HEAD case.
#[tauri::command]
pub fn checkout_commit_safe(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
) -> Result<CheckoutResult, AppError> {
    let oid_for_checkout = oid.clone();
    let short = oid.get(..7).unwrap_or(&oid).to_string();
    let result = safe_checkout_with_stash(
        &state,
        &tab_id,
        &format!("checkout commit {short} (detached, safe)"),
        |r| {
            crate::git::branch::checkout_commit_detached(r, &oid_for_checkout)?;
            Ok(None)
        },
    )?;

    if checkout_is_clean(&result) {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "oid": &oid });
            let _ = host.fire_hook("on_checkout", &ctx.to_string());
        }
    }

    Ok(result)
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
