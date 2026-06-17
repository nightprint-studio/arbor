//! `stash` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::stash`] (so the headless `corvus-be`
//! shares it). This module keeps the original shell-facing API — same
//! signatures, `AppError` results — so the in-process consumers (checkout-safe
//! stashing, graph stash markers, pull-with-stash, linked-worktree sync, and the
//! stash IPC handlers) are untouched. It injects the shell's resolved git program
//! (`GitCli`) and binds the **recovery snapshot** (which stays shell-side, in
//! `crate::git::recovery`) to the crate's snapshot callback.
//!
//! When `corvus-be` serves stash out-of-process (next step), the backend will
//! call `corvus_git::stash` directly with its own `GitCli` + a recovery binding.

use std::path::Path;

use git2::Repository;

use corvus_git::prelude::GitCli;

use crate::error::Result;
use crate::git::recovery::{try_snapshot, RecoveryKind};

// Re-export the data types so existing `crate::git::stash::Stash*` paths resolve.
pub use corvus_git::prelude::{StashApplyResult, StashBlockingContent, StashEntry, StashRef};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// Recovery snapshot binding for the abort path (`RecoveryKind::Other`).
fn snapshot_other(repo: &Repository, summary: &str) {
    try_snapshot(repo, RecoveryKind::Other, summary);
}

pub fn list_stashes(repo: &mut Repository) -> Result<Vec<StashEntry>> {
    Ok(corvus_git::stash::list_stashes(repo)?)
}

pub fn collect_stash_refs(repo: &mut Repository) -> Result<Vec<StashRef>> {
    Ok(corvus_git::stash::collect_stash_refs(repo)?)
}

pub fn stash_save(workdir: &Path, message: Option<&str>, include_untracked: bool) -> Result<StashEntry> {
    Ok(corvus_git::stash::stash_save(&git(), workdir, message, include_untracked)?)
}

pub fn stash_apply(repo: &mut Repository, index: usize) -> Result<StashApplyResult> {
    Ok(corvus_git::stash::stash_apply(&git(), repo, index)?)
}

pub fn stash_pop(repo: &mut Repository, index: usize) -> Result<StashApplyResult> {
    Ok(corvus_git::stash::stash_pop(&git(), repo, index)?)
}

pub fn stash_drop(repo: &mut Repository, index: usize) -> Result<()> {
    Ok(corvus_git::stash::stash_drop(&git(), repo, index)?)
}

pub fn stash_rename(repo: &mut Repository, index: usize, new_message: &str) -> Result<StashEntry> {
    Ok(corvus_git::stash::stash_rename(repo, index, new_message)?)
}

pub fn abort_stash_apply(workdir: &Path) -> Result<()> {
    Ok(corvus_git::stash::abort_stash_apply(&git(), workdir, &snapshot_other)?)
}

pub fn abort_stash_apply_with_snapshot(workdir: &Path, repo: Option<&Repository>) -> Result<()> {
    Ok(corvus_git::stash::abort_stash_apply_with_snapshot(&git(), workdir, repo, &snapshot_other)?)
}

pub fn force_stash_apply(
    repo: &mut Repository,
    index: usize,
    files_to_delete: &[String],
    files_to_keep: &[String],
    drop_on_success: bool,
) -> Result<StashApplyResult> {
    let snapshot = |r: &Repository, summary: &str| {
        try_snapshot(r, RecoveryKind::StashForceApply, summary);
    };
    Ok(corvus_git::stash::force_stash_apply(
        &git(), &snapshot, repo, index, files_to_delete, files_to_keep, drop_on_success,
    )?)
}

pub fn get_stash_file_content(
    repo: &Repository,
    index: usize,
    path: &str,
    encoding_override: Option<&str>,
) -> Result<StashBlockingContent> {
    Ok(corvus_git::stash::get_stash_file_content(repo, index, path, encoding_override)?)
}
