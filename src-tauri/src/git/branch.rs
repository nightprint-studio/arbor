//! `branch` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The branch/tag logic moved into [`corvus_git::branch`] (so the headless
//! `corvus-be` shares it). This module keeps the original shell-facing API —
//! same signatures, `AppError` results — so every in-process consumer (the
//! branch IPC handlers, GitFlow, the sidebar branch list, merged-branch
//! pruning, remote rename/delete flows) is untouched. It injects two couplings
//! the crate deliberately refuses:
//!
//! - the **recovery snapshot** (`try_snapshot`, which loads the policy from the
//!   app config and resolves the `GitCli` — both shell-only) is bound to the
//!   crate's `snapshot` closure with a fixed `RecoveryKind::Checkout`;
//! - the **credential-coupled push** (`crate::git::remote::push`, keyring +
//!   smart-HTTP auth) is bound to the crate's `push` closure.

use git2::Repository;

use crate::error::Result;
use crate::git::recovery::{try_snapshot, RecoveryKind};

// Re-export the data types so existing `crate::git::branch::*` paths resolve.
pub use corvus_git::prelude::{BranchInfo, RemoteRenameResult, TagInfo};

/// Recovery snapshot binding for the checkout paths (`RecoveryKind::Checkout`).
fn snapshot_checkout(repo: &Repository, summary: &str) {
    try_snapshot(repo, RecoveryKind::Checkout, summary.to_string());
}

/// Credential-coupled push binding. Keyring resolution + smart-HTTP auth live in
/// `crate::git::remote::push`; the `e.to_string()` preserves the exact `AppError`
/// `Display` string the crate's branch logic pattern-matches and re-surfaces.
fn push_remote(repo: &Repository, remote: &str, refspec: &str, force: bool) -> std::result::Result<(), String> {
    crate::git::remote::push(repo, remote, refspec, force).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub fn list_local_branches(repo: &Repository) -> Result<Vec<BranchInfo>> {
    Ok(corvus_git::branch::list_local_branches(repo)?)
}

pub fn list_remote_branches(repo: &Repository) -> Result<Vec<BranchInfo>> {
    Ok(corvus_git::branch::list_remote_branches(repo)?)
}

pub fn list_tags(repo: &Repository) -> Result<Vec<TagInfo>> {
    Ok(corvus_git::branch::list_tags(repo)?)
}

pub fn get_nearest_tag(repo: &Repository) -> Option<String> {
    corvus_git::branch::get_nearest_tag(repo)
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

pub fn create_branch(repo: &Repository, name: &str, from_oid: &str) -> Result<BranchInfo> {
    Ok(corvus_git::branch::create_branch(repo, name, from_oid)?)
}

pub fn delete_branch(repo: &Repository, name: &str) -> Result<()> {
    Ok(corvus_git::branch::delete_branch(repo, name)?)
}

pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<BranchInfo> {
    Ok(corvus_git::branch::rename_branch(repo, old_name, new_name)?)
}

pub fn checkout_branch(repo: &Repository, name: &str) -> Result<()> {
    Ok(corvus_git::branch::checkout_branch(repo, name, &snapshot_checkout)?)
}

pub fn checkout_remote_as_local(repo: &Repository, full_remote_name: &str) -> Result<String> {
    Ok(corvus_git::branch::checkout_remote_as_local(repo, full_remote_name, &snapshot_checkout)?)
}

pub fn checkout_commit_detached(repo: &Repository, oid_str: &str) -> Result<()> {
    Ok(corvus_git::branch::checkout_commit_detached(repo, oid_str, &snapshot_checkout)?)
}

pub fn list_merged_branches(repo: &Repository, target: &str) -> Result<Vec<BranchInfo>> {
    Ok(corvus_git::branch::list_merged_branches(repo, target)?)
}

pub fn list_merged_remote_branches(repo: &Repository, target: &str) -> Result<Vec<BranchInfo>> {
    Ok(corvus_git::branch::list_merged_remote_branches(repo, target)?)
}

pub fn delete_remote_branches(repo: &Repository, names: &[String]) -> Vec<String> {
    corvus_git::branch::delete_remote_branches(repo, names, &push_remote)
}

pub fn rename_remote_branch(
    repo: &Repository,
    full_remote_old_name: &str,
    new_short_name: &str,
    rename_local: bool,
) -> Result<RemoteRenameResult> {
    Ok(corvus_git::branch::rename_remote_branch(
        repo, full_remote_old_name, new_short_name, rename_local, &push_remote,
    )?)
}

pub fn delete_branches(repo: &Repository, names: &[String]) -> Vec<String> {
    corvus_git::branch::delete_branches(repo, names)
}
