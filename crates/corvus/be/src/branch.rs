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
//! Left in-process (need state `CorvusState` lacks):
//! - `get_status` — reads the `status.detect_renames` config + returns a
//!   potentially large hot-path payload (transport evaluation, like diff/graph).
//! - `create_branch` — checks the worktree-link **alias registry** for name
//!   conflicts.
//! - every worktree-link-aware checkout/delete/rename — the `WorktreeLinkRegistry`
//!   + the checkout-sync orchestrator live in the shell's `AppState`.

use corvus_core::prelude::CorvusState;
use corvus_git::branch::{BranchInfo, RemoteRenameResult, TagInfo};
use corvus_git::recovery::RecoveryKind;
use git2::Repository;
use serde_json::json;

use crate::remote::credential_resolver;
use crate::repo::{git, open, snapshot_policy};

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
