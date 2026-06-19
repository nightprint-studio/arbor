//! `branch` domain — the **not-yet-migrated** slice, still served in-process.
//!
//! The read queries, the remote-push delete/rename, the detached
//! `checkout_commit`, and `get_status` moved to `corvus-be`. What remains here
//! are the **worktree-link-aware** mutators (`create_branch`, `delete_branch`,
//! `rename_branch`, `checkout_branch`, `checkout_branch_safe`,
//! `checkout_remote_as_local`, `checkout_remote_as_local_safe`): they consult the
//! shell's `WorktreeLinkRegistry` (alias checks) and drive the cross-repo
//! checkout-sync orchestrator with `&AppState`, so they stay until that registry
//! moves to `CorvusState`. They emit `arbor://worktree-links-changed` and fire
//! their fire-and-forget hooks (`on_branch_create`, …) inline. The shared
//! checkout infrastructure (`CheckoutResult`, `safe_checkout_with_stash`,
//! `checkout_is_clean`, `repo_id_for_tab`) lives here too — these handlers are
//! its only consumers.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;
use crate::git::branch::BranchInfo;
use crate::git::stash::StashEntry;
use crate::ipc::corvus;
use crate::AppState;
use crate::linked_worktrees;

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
    // Fire inline, after the repos lock scope has dropped (Lua hooks may run git
    // ops; firing under a held lock would deadlock).
    state.fire_hook(
        "on_branch_create",
        json!({ "tab_id": tab_id, "name": name, "from_oid": from_oid }),
    );
    Ok(info)
}

// ---------------------------------------------------------------------------
// Commit checkout (detached — no AppHandle, no worktree-link sync)
// ---------------------------------------------------------------------------

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
    // Fire inline (lock already released inside `safe_checkout_with_stash`).
    // Gate on a clean result: no stash re-apply error AND no stash conflicts —
    // the typed equivalents of the post_hooks `clean_checkout(result)` predicate
    // (`stash_apply_error` is null ⇔ `is_none()`).
    if result.stash_apply_error.is_none() && result.stash_conflicts.is_empty() {
        state.fire_hook("on_checkout", json!({ "tab_id": tab_id, "oid": oid }));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Shared checkout infrastructure (local branch / remote-as-local, with
// worktree-link awareness). Used by the handlers below and by
// `checkout_commit_safe` above.
// ---------------------------------------------------------------------------

/// Look up the RepoRegistry UUID for a given tab.  Returns None if the tab
/// is not registered (defensive — every opened tab should be).
pub(crate) fn repo_id_for_tab(state: &AppState, tab_id: &str) -> Option<String> {
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

/// Returned by every `checkout_*_safe` handler so the frontend knows whether a
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
/// `do_checkout` performs the actual ref/HEAD move on a `&mut Repository` and
/// optionally returns a resolved local branch name (used by
/// `checkout_remote_as_local_safe`). The recovery snapshot is taken inside the
/// inner `branch::checkout_*` helpers — not here — so we don't pay for it twice.
///
/// Mirrors the pull auto-stash dance (stash → mutate → apply, never pop, so
/// the stash survives conflicts and apply errors).
pub(crate) fn safe_checkout_with_stash<F>(
    state: &AppState,
    tab_id: &str,
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
    // The recovery snapshot is taken inside the inner `branch::checkout_*`
    // helpers (called via `do_checkout`), so we don't take a second one here.
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(tab_id)?;
    let r = repo.inner_mut();

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
pub(crate) fn checkout_is_clean(r: &CheckoutResult) -> bool {
    r.stash_apply_error.is_none() && r.stash_conflicts.is_empty()
}

// ---------------------------------------------------------------------------
// Local branch / remote-as-local checkout + delete/rename — worktree-link aware
// ---------------------------------------------------------------------------

#[corvus::handler]
fn delete_branch(state: &AppState, tab_id: String, name: String) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::delete_branch(repo.inner(), &name)?;
    }
    state.fire_hook(
        "on_branch_delete",
        json!({ "tab_id": &tab_id, "name": &name }),
    );
    // Remove alias entries that referenced this (repo_id, branch).
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        if let Ok(mut reg) = state.lock_linked_worktrees() {
            let mut all = reg.list();
            let removed = linked_worktrees::aliases::on_branch_deleted(&mut all, &repo_id, &name);
            if removed > 0 {
                reg.replace_all(all);
                let _ = linked_worktrees::save(&reg);
                state.emit("arbor://worktree-links-changed", json!({}));
            }
        }
    }
    Ok(())
}

#[corvus::handler]
fn rename_branch(
    state: &AppState,
    tab_id: String,
    old_name: String,
    new_name: String,
) -> Result<BranchInfo, AppError> {
    let info = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::rename_branch(repo.inner(), &old_name, &new_name)?
    };
    state.fire_hook(
        "on_branch_rename",
        json!({ "tab_id": &tab_id, "old_name": &old_name, "new_name": &new_name }),
    );
    // Smart-update alias entries: rename in-place; collapse groups that
    // become trivial (every member shares the same branch name).
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        if let Ok(mut reg) = state.lock_linked_worktrees() {
            let mut all = reg.list();
            let affected = linked_worktrees::aliases::on_branch_renamed(&mut all, &repo_id, &old_name, &new_name);
            if affected > 0 {
                reg.replace_all(all);
                let _ = linked_worktrees::save(&reg);
                state.emit("arbor://worktree-links-changed", json!({}));
            }
        }
    }
    Ok(info)
}

#[corvus::handler]
fn checkout_branch(state: &AppState, tab_id: String, name: String) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::checkout_branch(repo.inner(), &name)?;
    }
    state.fire_hook(
        "on_checkout",
        json!({ "tab_id": &tab_id, "branch": &name }),
    );
    // Trigger space sync if this tab's repo is a member of a sync-enabled
    // space.  No-op otherwise.  Runs in a background thread.
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        linked_worktrees::orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, &name);
    }
    Ok(())
}

/// Stash-safe branch checkout: stash dirty workdir → checkout branch → stash apply.
/// Uses stash_apply (never pop) so the stash is preserved on conflict or error.
#[corvus::handler]
fn checkout_branch_safe(
    state: &AppState,
    tab_id: String,
    name: String,
) -> Result<CheckoutResult, AppError> {
    let name_for_checkout = name.clone();
    let result = safe_checkout_with_stash(state, &tab_id, |r| {
        crate::git::branch::checkout_branch(r, &name_for_checkout)?;
        Ok(None)
    })?;

    // Fire hook + trigger worktree-link sync only on clean success.
    if checkout_is_clean(&result) {
        state.fire_hook(
            "on_checkout",
            json!({ "tab_id": &tab_id, "branch": &name }),
        );
        if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
            linked_worktrees::orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, &name);
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
#[corvus::handler]
fn checkout_remote_as_local(
    state: &AppState,
    tab_id: String,
    remote_name: String,
) -> Result<String, AppError> {
    let local_name = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::branch::checkout_remote_as_local(repo.inner(), &remote_name)?
    };
    state.fire_hook(
        "on_checkout",
        json!({ "tab_id": &tab_id, "branch": &local_name }),
    );
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        linked_worktrees::orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, &local_name);
    }
    Ok(local_name)
}

/// Stash-safe variant of `checkout_remote_as_local`: stash dirty workdir →
/// create local tracking branch (if missing) → switch HEAD → re-apply stash.
/// The resolved local branch short name is returned in `resolved_local_name`
/// even when stash apply produces conflicts, so the tab badge can be updated.
#[corvus::handler]
fn checkout_remote_as_local_safe(
    state: &AppState,
    tab_id: String,
    remote_name: String,
) -> Result<CheckoutResult, AppError> {
    let remote_for_checkout = remote_name.clone();
    let result = safe_checkout_with_stash(state, &tab_id, |r| {
        let local = crate::git::branch::checkout_remote_as_local(r, &remote_for_checkout)?;
        Ok(Some(local))
    })?;

    if checkout_is_clean(&result) {
        if let Some(ref local_name) = result.resolved_local_name {
            state.fire_hook(
                "on_checkout",
                json!({ "tab_id": &tab_id, "branch": local_name }),
            );
            if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
                linked_worktrees::orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, local_name);
            }
        }
    }

    Ok(result)
}
