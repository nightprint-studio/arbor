//! `branch` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The pure
//! git work lives in [`crate::git::branch`] / [`crate::git::status`]; these
//! handlers are the thin `AppState` shell around it. Behavior (locks held,
//! errors) is byte-identical.
//!
//! Hooks: a few of these methods fired fire-and-forget plugin hooks inline
//! (`on_branch_create`, `on_branch_delete`). Those inline fires are **removed**
//! here — they now fire from `post_hooks.rs` in the generic `rpc` path so they
//! run exactly once regardless of in/out-of-process dispatch. See `post_hooks`.
//!
//! NOT migrated (stay inline in `branch_commands`, handled by a later
//! emit/seam pass): every command taking an `AppHandle` — `delete_branch`,
//! `rename_branch`, `checkout_branch`, `checkout_branch_safe`,
//! `checkout_remote_as_local`, `checkout_remote_as_local_safe`. They either emit
//! `arbor://worktree-links-changed` or call the worktree-link checkout-sync
//! orchestrator with the `AppHandle`. The shared checkout infrastructure
//! (`CheckoutResult`, `safe_checkout_with_stash`, `checkout_is_clean`,
//! `repo_id_for_tab`) lives in `branch_commands` and is reused from here.

use crate::error::AppError;
use crate::git::branch::{BranchInfo, RemoteRenameResult, TagInfo};
use crate::git::status::RepoStatus;
use crate::ipc::corvus;
use crate::AppState;
use crate::linked_worktrees;

use crate::commands::branch_commands::{
    repo_id_for_tab, safe_checkout_with_stash, CheckoutResult,
};

// ---------------------------------------------------------------------------
// Read-only
// ---------------------------------------------------------------------------

#[corvus::handler]
fn get_status(state: &AppState, tab_id: String) -> Result<RepoStatus, AppError> {
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

#[corvus::handler]
fn list_local_branches(state: &AppState, tab_id: String) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_local_branches(repo.inner())
}

#[corvus::handler]
fn list_remote_branches(state: &AppState, tab_id: String) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_remote_branches(repo.inner())
}

#[corvus::handler]
fn list_tags(state: &AppState, tab_id: String) -> Result<Vec<TagInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_tags(repo.inner())
}

#[corvus::handler]
fn get_nearest_tag(state: &AppState, tab_id: String) -> Result<Option<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(crate::git::branch::get_nearest_tag(repo.inner()))
}

#[corvus::handler]
fn list_merged_branches(
    state: &AppState,
    tab_id: String,
    target: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_merged_branches(repo.inner(), &target)
}

#[corvus::handler]
fn list_merged_remote_branches(
    state: &AppState,
    tab_id: String,
    target: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::list_merged_remote_branches(repo.inner(), &target)
}

// ---------------------------------------------------------------------------
// Create / delete / rename (local + remote, no AppHandle)
// ---------------------------------------------------------------------------

// Fires `on_branch_create` — now from `post_hooks.rs`, not here.
#[corvus::handler]
fn create_branch(
    state: &AppState,
    tab_id: String,
    name: String,
    from_oid: String,
) -> Result<BranchInfo, AppError> {
    // Refuse names that would conflict with an active alias mapping in any
    // space this repo belongs to.  The user must remove the alias first.
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        if let Ok(reg) = state.lock_linked_worktrees() {
            let all = reg.list();
            if let Some(link_name) =
                linked_worktrees::aliases::alias_blocks_branch_name(&all, &repo_id, &name)
            {
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
    Ok(info)
}

// Fires `on_branch_delete` — now from `post_hooks.rs`, not here. The deleted
// names are returned so post_hooks can read them off the result.
#[corvus::handler]
fn delete_remote_branches(
    state: &AppState,
    tab_id: String,
    names: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let deleted_names: Vec<String> = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let failed = crate::git::branch::delete_remote_branches(repo.inner(), &names);
        names.iter().filter(|n| !failed.contains(n)).cloned().collect()
    };
    // Return the failed names (same convention as delete_branches)
    let failed: Vec<String> =
        names.into_iter().filter(|n| !deleted_names.contains(n)).collect();
    Ok(failed)
}

#[corvus::handler]
fn rename_remote_branch(
    state: &AppState,
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
    // Fires `on_branch_rename` — now from `post_hooks.rs`, not here.
    Ok(result)
}

// Fires `on_branch_delete` — now from `post_hooks.rs`, not here. The returned
// vec is the list of branches actually deleted, which post_hooks reads.
#[corvus::handler]
fn delete_branches(
    state: &AppState,
    tab_id: String,
    names: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let deleted = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::delete_branches(repo.inner(), &names)
    };
    Ok(deleted)
}

// ---------------------------------------------------------------------------
// Commit checkout (detached — no AppHandle, no worktree-link sync)
// ---------------------------------------------------------------------------

/// Non-safe commit checkout — kept for backward compat. Errors out on dirty
/// workdir (libgit2 Conflict). New callers should use `checkout_commit_safe`.
///
/// Fires `on_checkout` — now from `post_hooks.rs`, not here.
#[corvus::handler]
fn checkout_commit(state: &AppState, tab_id: String, oid: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::branch::checkout_commit_detached(repo.inner(), &oid)?;
    Ok(())
}

/// Stash-safe detached commit checkout: stash dirty workdir → detach HEAD →
/// re-apply stash. Mirrors `checkout_branch_safe` for the detached-HEAD case.
///
/// Fires `on_checkout` (only on a clean result) — now from `post_hooks.rs`,
/// which gates on the same clean predicate.
#[corvus::handler]
fn checkout_commit_safe(
    state: &AppState,
    tab_id: String,
    oid: String,
) -> Result<CheckoutResult, AppError> {
    let oid_for_checkout = oid.clone();
    let result = safe_checkout_with_stash(state, &tab_id, |r| {
        crate::git::branch::checkout_commit_detached(r, &oid_for_checkout)?;
        Ok(None)
    })?;
    // The `on_checkout` hook (gated on a clean result) now fires from
    // `post_hooks.rs`, so nothing else to do here.
    Ok(result)
}
