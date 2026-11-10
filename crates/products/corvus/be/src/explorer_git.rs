//! Git awareness for the File Explorer (sitta), by **path** — badges, change list, branches and the
//! light inline actions.
//!
//! Sitta owns no git of its own: the explorer borrows Corvus the way Bennu does (`ensureBackend('corvus')`),
//! and only when the git switch is on and the folder sits inside a repository. These are thin
//! `#[arbor_rpc::handler]` wrappers over [`corvus_git::explorer`] (the pure git2 logic); like
//! `tree_status`, no tab is involved — the repository is discovered from the path.
//!
//! `fs_git_discard` snapshots to the repo's recovery journal first, with the backend's real git invoker
//! and the user's retention policy (so Corvus's Recovery tab can undo an over-eager discard).

use corvus_core::prelude::CorvusState;
use corvus_git::explorer;
use corvus_git::prelude::{FsBranch, FsGitStatus, GitChanges};

use crate::repo::{git, snapshot_policy};

/// Badges + branch + ahead/behind for the entries of `dir`. Cached per repo root; `refresh` recomputes.
#[arbor_rpc::handler]
fn fs_git_status(_state: &CorvusState, dir: String, refresh: Option<bool>) -> Result<FsGitStatus, String> {
    explorer::status(&dir, refresh.unwrap_or(false))
}

#[arbor_rpc::handler]
fn fs_git_changes(_state: &CorvusState, dir: String) -> Result<GitChanges, String> {
    Ok(explorer::changes(&dir))
}

#[arbor_rpc::handler]
fn fs_git_branches(_state: &CorvusState, path: String) -> Result<Vec<FsBranch>, String> {
    explorer::branches(&path)
}

#[arbor_rpc::handler]
fn fs_git_remote_url(_state: &CorvusState, path: String) -> Result<Option<String>, String> {
    explorer::remote_url(&path)
}

#[arbor_rpc::handler]
fn fs_git_stage(_state: &CorvusState, paths: Vec<String>) -> Result<(), String> {
    explorer::stage(&paths)
}

#[arbor_rpc::handler]
fn fs_git_unstage(_state: &CorvusState, paths: Vec<String>) -> Result<(), String> {
    explorer::unstage(&paths)
}

#[arbor_rpc::handler]
fn fs_git_discard(state: &CorvusState, paths: Vec<String>) -> Result<(), String> {
    let git = git(state);
    let policy = snapshot_policy(state);
    explorer::discard(&paths, Some((&git, &policy)))
}

#[arbor_rpc::handler]
fn fs_git_ignore(_state: &CorvusState, paths: Vec<String>) -> Result<(), String> {
    explorer::ignore(&paths)
}

#[arbor_rpc::handler]
fn fs_git_checkout(_state: &CorvusState, path: String, branch: String) -> Result<(), String> {
    explorer::checkout(&path, &branch)
}
