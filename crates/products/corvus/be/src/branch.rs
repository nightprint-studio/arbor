//! `branch` domain (partial) — read-only branch/tag queries + the
//! worktree-link-free mutators, served **out-of-process** by corvus-be.
//!
//! The 6 reads + local `delete_branches` + the detached `checkout_commit` run
//! here: pure `corvus-git` calls on the repo opened by the pushed path.
//! `delete_branches`/`checkout_commit` fire their fire-and-forget hooks inline at
//! the co-located host — `post_hooks` ignores `program == "corvus"`, so the
//! inline fire is the single fire (same discipline as the stash/reset OOP
//! domains; the stale "fires from post_hooks" note on the in-process copy
//! predates W0a).
//!
//! The two **remote-push** mutators (`delete_remote_branches`,
//! `rename_remote_branch`) run here too: their `corvus-git` logic takes an
//! injected `push` closure, bound to `corvus_git::remote::push` over the shared
//! `__git_credentials` reverse-channel resolver (the same git auth `remote` /
//! `notes` / `merge_mr` use). The blocking resolver round-trip runs on the
//! per-request worker thread (the reader thread delivers — no deadlock).
//!
//! The **worktree-link-aware** mutators run here too (`create_branch`,
//! `delete_branch`, `rename_branch`, `checkout_branch` / `_safe`,
//! `checkout_remote_as_local` / `_safe`, `checkout_commit_safe`): the registry +
//! the checkout-sync orchestrator now live in `corvus-be`
//! ([`crate::worktree_links`]). `create_branch` refuses names reserved by an
//! alias group; `delete`/`rename` clean up / smart-rename alias entries; the
//! checkout handlers fire `on_checkout` (the `_safe` variants only on a clean
//! result) and trigger `orchestrator::maybe_trigger_checkout_sync`. The
//! stash-safe core ([`safe_checkout_with_stash`]) opens the repo by the pushed
//! path, stashes the dirty workdir over the shared `corvus-git` stash, runs the
//! injected checkout, and re-applies (never pops). `repo_id_for_tab` resolves a
//! tab to its RepoRegistry UUID via the pushed `repo_registry` (path-matched the
//! same way the shell's `find_by_path` does).

use corvus_core::prelude::CorvusState;
use corvus_git::branch::{BranchInfo, RemoteRenameResult, TagInfo};
use corvus_git::recovery::RecoveryKind;
use corvus_git::stash::StashEntry;
use git2::Repository;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::remote::credential_resolver;
use crate::repo::{git, open, repo_path, snapshot_policy};
use crate::worktree_links::{self, orchestrator};

// ── Read-only ────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn list_local_branches(state: &CorvusState, tab_id: String) -> Result<Vec<BranchInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::branch::list_local_branches(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn list_remote_branches(state: &CorvusState, tab_id: String) -> Result<Vec<BranchInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::branch::list_remote_branches(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn list_tags(state: &CorvusState, tab_id: String) -> Result<Vec<TagInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::branch::list_tags(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_nearest_tag(state: &CorvusState, tab_id: String) -> Result<Option<String>, String> {
    let repo = open(state, &tab_id)?;
    Ok(corvus_git::branch::get_nearest_tag(&repo))
}

#[arbor_rpc::handler]
fn list_merged_branches(
    state: &CorvusState,
    tab_id: String,
    target: String,
) -> Result<Vec<BranchInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::branch::list_merged_branches(&repo, &target).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn list_merged_remote_branches(
    state: &CorvusState,
    tab_id: String,
    target: String,
) -> Result<Vec<BranchInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::branch::list_merged_remote_branches(&repo, &target).map_err(|e| e.to_string())
}

// ── Local delete (no worktree-link alias coupling) ───────────────────────────

#[arbor_rpc::handler]
fn delete_branches(
    state: &CorvusState,
    tab_id: String,
    names: Vec<String>,
) -> Result<Vec<String>, String> {
    let deleted = {
        let repo = open(state, &tab_id)?;
        corvus_git::branch::delete_branches(&repo, &names)
    };
    if !deleted.is_empty() {
        state.fire_hook("on_branch_delete", json!({ "tab_id": tab_id, "names": deleted }));
    }
    Ok(deleted)
}

// ── Remote-push delete / rename (git-push credentials over `__git_credentials`) ──

#[arbor_rpc::handler]
fn delete_remote_branches(
    state: &CorvusState,
    tab_id: String,
    names: Vec<String>,
) -> Result<Vec<String>, String> {
    let deleted_names: Vec<String> = {
        let repo = open(state, &tab_id)?;
        let host = state
            .host_caller()
            .ok_or_else(|| "delete_remote_branches: no reverse channel for credentials".to_string())?;
        let resolver = credential_resolver(host);
        let push = move |repo: &Repository, remote: &str, refspec: &str, force: bool| {
            corvus_git::remote::push(repo, remote, refspec, force, &resolver).map_err(|e| e.to_string())
        };
        let failed = corvus_git::branch::delete_remote_branches(&repo, &names, &push);
        names.iter().filter(|n| !failed.contains(n)).cloned().collect()
    };
    if !deleted_names.is_empty() {
        state.fire_hook("on_branch_delete", json!({ "tab_id": tab_id, "names": deleted_names }));
    }
    let failed: Vec<String> =
        names.into_iter().filter(|n| !deleted_names.contains(n)).collect();
    Ok(failed)
}

#[arbor_rpc::handler]
fn rename_remote_branch(
    state: &CorvusState,
    tab_id: String,
    old_full_name: String,
    new_short_name: String,
    rename_local: bool,
) -> Result<RemoteRenameResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let host = state
            .host_caller()
            .ok_or_else(|| "rename_remote_branch: no reverse channel for credentials".to_string())?;
        let resolver = credential_resolver(host);
        let push = move |repo: &Repository, remote: &str, refspec: &str, force: bool| {
            corvus_git::remote::push(repo, remote, refspec, force, &resolver).map_err(|e| e.to_string())
        };
        corvus_git::branch::rename_remote_branch(
            &repo,
            &old_full_name,
            &new_short_name,
            rename_local,
            &push,
        )
        .map_err(|e| e.to_string())?
    };
    state.fire_hook(
        "on_branch_rename",
        json!({
            "tab_id": tab_id,
            "old_name": old_full_name,
            "new_name": result.new_full_name,
            "local_renamed": result.local_renamed,
        }),
    );
    Ok(result)
}

// ── Detached commit checkout (no worktree-link sync) ─────────────────────────

#[arbor_rpc::handler]
fn checkout_commit(state: &CorvusState, tab_id: String, oid: String) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        // Inject the recovery snapshot the crate takes before detaching HEAD —
        // the shell-pushed policy + this backend's git program, fixed
        // `RecoveryKind::Checkout` (mirrors the shell's `snapshot_checkout`).
        let g = git(state);
        let policy = snapshot_policy(state);
        let snapshot = |repo: &Repository, summary: &str| {
            let _ = corvus_git::recovery::snapshot_with_policy(
                &g,
                repo,
                RecoveryKind::Checkout,
                summary,
                &policy,
            );
        };
        corvus_git::branch::checkout_commit_detached(&repo, &oid, &snapshot)
            .map_err(|e| e.to_string())?;
    }
    state.fire_hook("on_checkout", json!({ "tab_id": tab_id, "oid": oid }));
    Ok(())
}

// ── Worktree-link-aware mutators ──────────────────────────────────────────────

/// Resolve a tab to its RepoRegistry UUID via corvus-be's own repo registry
/// (`find_by_path` applies the same separator + Windows-case normalisation).
/// `None` if the tab isn't registered → the worktree-link side effects don't fire.
fn repo_id_for_tab(state: &CorvusState, tab_id: &str) -> Option<String> {
    let path = repo_path(state, tab_id).ok()?;
    crate::workspace::registry::registry(state)
        .find_by_path(&path)
        .map(|e| e.id.clone())
}

/// Run `f` with the recovery-snapshot closure the checkout helpers inject — the
/// OOP twin of the shell's `snapshot_checkout` (`RecoveryKind::Checkout`,
/// shell-pushed policy + this backend's git program).
fn with_checkout_snapshot<T>(state: &CorvusState, f: impl FnOnce(&dyn Fn(&Repository, &str)) -> T) -> T {
    let g = git(state);
    let policy = snapshot_policy(state);
    let snapshot = |repo: &Repository, summary: &str| {
        let _ = corvus_git::recovery::snapshot_with_policy(&g, repo, RecoveryKind::Checkout, summary, &policy);
    };
    f(&snapshot)
}

/// Returned by every `checkout_*_safe` handler so the FE knows whether a
/// pre-checkout stash needs re-applying and whether that had conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutResult {
    pub stash_conflicts: Vec<String>,
    pub pre_checkout_stash: Option<StashEntry>,
    /// Non-None when stash re-apply failed for a non-conflict reason; the stash
    /// entry is preserved at index 0 (user applies it manually).
    pub stash_apply_error: Option<String>,
    /// The short name of the local branch created/reused for a remote ref
    /// (`checkout_remote_as_local_safe`); `None` for branch/commit checkouts.
    #[serde(default)]
    pub resolved_local_name: Option<String>,
    /// True when a pre-checkout stash was created (survives the clean-apply path
    /// where `pre_checkout_stash` is cleared after drop).
    #[serde(default)]
    pub did_stash: bool,
}

/// True when a `CheckoutResult` is fully clean (no re-apply error, no conflicts)
/// — gates hook firing + worktree-link sync.
fn checkout_is_clean(r: &CheckoutResult) -> bool {
    r.stash_apply_error.is_none() && r.stash_conflicts.is_empty()
}

/// Stash-safe checkout core: stash dirty workdir → run `do_checkout` → re-apply
/// the stash (never pop, so it survives conflicts/errors). The recovery snapshot
/// is taken inside the inner `branch::checkout_*` helpers (via `do_checkout`).
fn safe_checkout_with_stash<F>(
    state: &CorvusState,
    tab_id: &str,
    do_checkout: F,
) -> Result<CheckoutResult, String>
where
    F: FnOnce(&mut Repository) -> Result<Option<String>, String>,
{
    let git = git(state);
    let repo_path = repo_path(state, tab_id)?;
    let mut r = Repository::open(&repo_path).map_err(|e| e.to_string())?;

    let workdir = r
        .workdir()
        .ok_or_else(|| "bare repository".to_string())?
        .to_path_buf();
    let is_dirty = {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        let statuses = r.statuses(Some(&mut opts)).map_err(|e| e.to_string())?;
        statuses.iter().any(|s| s.status() != git2::Status::CURRENT)
    };

    // CLI stash — does not hold a git2 handle.
    let stash_entry: Option<StashEntry> = if is_dirty {
        let entry = corvus_git::stash::stash_save(&git, &workdir, Some("arbor: pre-checkout stash"), true)
            .map_err(|e| format!("stash failed: {e}"))?;
        Some(entry)
    } else {
        None
    };
    let did_stash = stash_entry.is_some();

    // Catch checkout failures explicitly so the preserved-stash context survives
    // to the FE (a plain `?` would lose it).
    let resolved_local_name = match do_checkout(&mut r) {
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
        match corvus_git::stash::stash_apply(&git, &mut r, 0) {
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

#[arbor_rpc::handler]
fn create_branch(state: &CorvusState, tab_id: String, name: String, from_oid: String) -> Result<BranchInfo, String> {
    // Refuse names reserved by an active alias mapping in any link this repo
    // belongs to (the user must remove the alias first).
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        let all = worktree_links::registry(state).list();
        if let Some(link_name) = worktree_links::aliases::alias_blocks_branch_name(&all, &repo_id, &name) {
            return Err(format!(
                "branch '{name}' is reserved by an alias in worktree link '{link_name}' — remove the alias to free this name"
            ));
        }
    }
    let info = {
        let repo = open(state, &tab_id)?;
        corvus_git::branch::create_branch(&repo, &name, &from_oid).map_err(|e| e.to_string())?
    };
    state.fire_hook("on_branch_create", json!({ "tab_id": tab_id, "name": name, "from_oid": from_oid }));
    Ok(info)
}

#[arbor_rpc::handler]
fn delete_branch(state: &CorvusState, tab_id: String, name: String) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        corvus_git::branch::delete_branch(&repo, &name).map_err(|e| e.to_string())?;
    }
    state.fire_hook("on_branch_delete", json!({ "tab_id": &tab_id, "name": &name }));
    // Remove alias entries that referenced this (repo_id, branch).
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        let removed = worktree_links::mutate(state, |reg| {
            let mut all = reg.list();
            let n = worktree_links::aliases::on_branch_deleted(&mut all, &repo_id, &name);
            if n > 0 {
                reg.replace_all(all);
            }
            Ok(n)
        })?;
        if removed > 0 {
            state.emit("arbor://worktree-links-changed", json!({}));
        }
    }
    Ok(())
}

#[arbor_rpc::handler]
fn rename_branch(state: &CorvusState, tab_id: String, old_name: String, new_name: String) -> Result<BranchInfo, String> {
    let info = {
        let repo = open(state, &tab_id)?;
        corvus_git::branch::rename_branch(&repo, &old_name, &new_name).map_err(|e| e.to_string())?
    };
    state.fire_hook(
        "on_branch_rename",
        json!({ "tab_id": &tab_id, "old_name": &old_name, "new_name": &new_name }),
    );
    // Smart-update alias entries: rename in-place; collapse trivial groups.
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        let affected = worktree_links::mutate(state, |reg| {
            let mut all = reg.list();
            let n = worktree_links::aliases::on_branch_renamed(&mut all, &repo_id, &old_name, &new_name);
            if n > 0 {
                reg.replace_all(all);
            }
            Ok(n)
        })?;
        if affected > 0 {
            state.emit("arbor://worktree-links-changed", json!({}));
        }
    }
    Ok(info)
}

#[arbor_rpc::handler]
fn checkout_branch(state: &CorvusState, tab_id: String, name: String) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        with_checkout_snapshot(state, |snap| corvus_git::branch::checkout_branch(&repo, &name, snap))
            .map_err(|e| e.to_string())?;
    }
    state.fire_hook("on_checkout", json!({ "tab_id": &tab_id, "branch": &name }));
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, &name);
    }
    Ok(())
}

#[arbor_rpc::handler]
fn checkout_branch_safe(state: &CorvusState, tab_id: String, name: String) -> Result<CheckoutResult, String> {
    let name_for_checkout = name.clone();
    let result = safe_checkout_with_stash(state, &tab_id, |r| {
        with_checkout_snapshot(state, |snap| corvus_git::branch::checkout_branch(r, &name_for_checkout, snap))
            .map_err(|e| e.to_string())?;
        Ok(None)
    })?;
    if checkout_is_clean(&result) {
        state.fire_hook("on_checkout", json!({ "tab_id": &tab_id, "branch": &name }));
        if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
            orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, &name);
        }
    }
    Ok(result)
}

#[arbor_rpc::handler]
fn checkout_remote_as_local(state: &CorvusState, tab_id: String, remote_name: String) -> Result<String, String> {
    let local_name = {
        let repo = open(state, &tab_id)?;
        with_checkout_snapshot(state, |snap| corvus_git::branch::checkout_remote_as_local(&repo, &remote_name, snap))
            .map_err(|e| e.to_string())?
    };
    state.fire_hook("on_checkout", json!({ "tab_id": &tab_id, "branch": &local_name }));
    if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
        orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, &local_name);
    }
    Ok(local_name)
}

#[arbor_rpc::handler]
fn checkout_remote_as_local_safe(state: &CorvusState, tab_id: String, remote_name: String) -> Result<CheckoutResult, String> {
    let remote_for_checkout = remote_name.clone();
    let result = safe_checkout_with_stash(state, &tab_id, |r| {
        let local =
            with_checkout_snapshot(state, |snap| corvus_git::branch::checkout_remote_as_local(r, &remote_for_checkout, snap))
                .map_err(|e| e.to_string())?;
        Ok(Some(local))
    })?;
    if checkout_is_clean(&result) {
        if let Some(ref local_name) = result.resolved_local_name {
            state.fire_hook("on_checkout", json!({ "tab_id": &tab_id, "branch": local_name }));
            if let Some(repo_id) = repo_id_for_tab(state, &tab_id) {
                orchestrator::maybe_trigger_checkout_sync(state, &tab_id, &repo_id, local_name);
            }
        }
    }
    Ok(result)
}

#[arbor_rpc::handler]
fn checkout_commit_safe(state: &CorvusState, tab_id: String, oid: String) -> Result<CheckoutResult, String> {
    let oid_for_checkout = oid.clone();
    let result = safe_checkout_with_stash(state, &tab_id, |r| {
        with_checkout_snapshot(state, |snap| corvus_git::branch::checkout_commit_detached(r, &oid_for_checkout, snap))
            .map_err(|e| e.to_string())?;
        Ok(None)
    })?;
    if checkout_is_clean(&result) {
        state.fire_hook("on_checkout", json!({ "tab_id": tab_id, "oid": oid }));
    }
    Ok(result)
}
