//! `status` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The status logic moved into [`corvus_git::status`] (so the headless
//! `corvus-be` shares it). It was already pure libgit2, so this wrapper is a
//! straight pass-through: same signatures, `AppError` results — every in-process
//! consumer (the status IPC handler, the auto-refresh poller, commit/stage flows
//! that read `RepoStatus`) is untouched. The `impl From<GitError> for AppError`
//! maps the error variant-for-variant, keeping the wire string identical.

use git2::Repository;

use crate::error::Result;

// Re-export the data types so existing `crate::git::status::*` paths resolve.
pub use corvus_git::prelude::RepoStatus;

#[allow(dead_code)]
pub fn get_status(repo: &Repository) -> Result<RepoStatus> {
    Ok(corvus_git::status::get_status(repo)?)
}

pub fn get_status_with(repo: &Repository, detect_renames: bool) -> Result<RepoStatus> {
    Ok(corvus_git::status::get_status_with(repo, detect_renames)?)
}
