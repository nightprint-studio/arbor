//! Per-handler repo/git helpers shared by the domain modules.
//!
//! Every git domain served here resolves the same two things from `CorvusState`:
//! the repo for a `tab_id` (the shell pushes the path; there is no `RepoManager`)
//! and the git invoker (the program the shell pushed). Centralised so the error
//! string and the "open by pushed path" shape live in one place.

use std::path::PathBuf;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::GitCli;
use git2::Repository;

/// The git invoker for this backend (the program the shell pushed, else `git`).
pub fn git(state: &CorvusState) -> GitCli {
    GitCli::from_optional(state.git_program().map(PathBuf::from))
}

/// Resolve a tab to its repo path, or a clear error if the shell never
/// registered it (should not happen for an open tab). Used by domains that
/// shell out to the `git` CLI on a path (e.g. bisect) rather than open a handle.
pub fn repo_path(state: &CorvusState, tab_id: &str) -> Result<String, String> {
    state
        .repo_path(tab_id)
        .ok_or_else(|| format!("repo not registered for tab '{tab_id}'"))
}

/// Open the repo registered for `tab_id` as a libgit2 handle, or the same clear
/// error as [`repo_path`].
pub fn open(state: &CorvusState, tab_id: &str) -> Result<Repository, String> {
    Repository::open(repo_path(state, tab_id)?).map_err(|e| e.to_string())
}
