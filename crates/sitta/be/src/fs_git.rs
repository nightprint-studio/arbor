//! `fs_git` domain — git awareness for the built-in File Explorer, served by
//! sitta-be.
//!
//! Thin `#[arbor_rpc::handler]` wrappers over [`corvus_git::explorer`] (the pure,
//! shared git2 logic). The explorer browses arbitrary on-disk paths, not tab-bound
//! repos, so none of these touch `SittaState` — the macro requires a context first
//! arg, so every handler takes `_state: &SittaState` and ignores it.
//!
//! `fs_git_discard` opts into a recovery snapshot before discarding (the Recovery
//! safety net): it injects the system `git` invoker (`GitCli::from_optional(None)`)
//! and the built-in `SnapshotPolicy::default()`. sitta-be owns no per-repo
//! retention config, so the default policy is used — the snapshot still lands in
//! the repo's recovery journal, so Corvus's Recovery tab sees it regardless.
//!
//! NOT here (stays shell-side): `fs_open_in_arbor` — it needs an `AppHandle` to
//! focus the main window and emit `arbor://explorer-open-repo`. No hooks fire here.

use corvus_git::explorer;
use corvus_git::prelude::{FsBranch, FsGitStatus, GitChanges, GitCli, SnapshotPolicy};

use sitta_core::prelude::SittaState;

#[arbor_rpc::handler]
fn fs_git_status(
    _state: &SittaState,
    dir: String,
    refresh: Option<bool>,
) -> Result<FsGitStatus, String> {
    explorer::status(&dir, refresh.unwrap_or(false))
}

#[arbor_rpc::handler]
fn fs_git_changes(_state: &SittaState, dir: String) -> Result<GitChanges, String> {
    Ok(explorer::changes(&dir))
}

#[arbor_rpc::handler]
fn fs_git_branches(_state: &SittaState, path: String) -> Result<Vec<FsBranch>, String> {
    explorer::branches(&path)
}

#[arbor_rpc::handler]
fn fs_git_remote_url(_state: &SittaState, path: String) -> Result<Option<String>, String> {
    explorer::remote_url(&path)
}

#[arbor_rpc::handler]
fn fs_git_stage(_state: &SittaState, paths: Vec<String>) -> Result<(), String> {
    explorer::stage(&paths)
}

#[arbor_rpc::handler]
fn fs_git_unstage(_state: &SittaState, paths: Vec<String>) -> Result<(), String> {
    explorer::unstage(&paths)
}

#[arbor_rpc::handler]
fn fs_git_discard(_state: &SittaState, paths: Vec<String>) -> Result<(), String> {
    // Inject the system git invoker + default retention so the discard snapshots
    // to the repo's recovery journal first (the Recovery-tab safety net).
    let git = GitCli::from_optional(None);
    let policy = SnapshotPolicy::default();
    explorer::discard(&paths, Some((&git, &policy)))
}

#[arbor_rpc::handler]
fn fs_git_ignore(_state: &SittaState, paths: Vec<String>) -> Result<(), String> {
    explorer::ignore(&paths)
}

#[arbor_rpc::handler]
fn fs_git_checkout(_state: &SittaState, path: String, branch: String) -> Result<(), String> {
    explorer::checkout(&path, &branch)
}
