//! `bisect` domain — **in-process fallback**.
//!
//! `corvus-be` serves bisect out-of-process (see `crate::ipc::corvus_be` /
//! `docs/corvus-be-bringup.md`); when it's running the `SplitBroker` routes these
//! methods there and this copy is unused. It stays as the fallback for when
//! `corvus-be` isn't built/spawned, so bisect never breaks on a missing backend.
//!
//! Either way the git logic is the shared Tauri-free [`corvus_git`] crate (single
//! source of truth) — only the context differs: here the repo path comes from the
//! shell's `RepoManager`, in `corvus-be` from the shell-pushed registry.
//! `corvus_git`'s `GitError` maps to `AppError` (see `crate::error`) so the wire
//! string is identical to the out-of-process path.

use corvus_git::prelude::{BisectMark, BisectSession, BisectState, GitCli};

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

/// The shell's resolved git program, as a `corvus-git` invoker.
fn git_cli() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

#[corvus::handler]
fn bisect_start(state: &AppState, tab_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::bisect_start(&git_cli(), &repo.path)?)
}

#[corvus::handler]
fn bisect_mark(state: &AppState, tab_id: String, hash: String, mark: String) -> Result<BisectState, AppError> {
    let mark = match mark.as_str() {
        "good" => BisectMark::Good,
        "bad" => BisectMark::Bad,
        "skip" => BisectMark::Skip,
        other => return Err(AppError::Other(format!("unknown bisect mark: {other}"))),
    };
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::bisect_mark(&git_cli(), &repo.path, &hash, mark)?)
}

#[corvus::handler]
fn bisect_reset(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::bisect_reset(&git_cli(), &repo.path)?)
}

#[corvus::handler]
fn get_bisect_state(state: &AppState, tab_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::get_bisect_state(&repo.path)?)
}

#[corvus::handler]
fn bisect_undo_last_mark(state: &AppState, tab_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::bisect_undo_last_mark(&git_cli(), &repo.path)?)
}

#[corvus::handler]
fn list_bisect_sessions(state: &AppState, tab_id: String) -> Result<Vec<BisectSession>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::list_sessions(&repo.path)?)
}

#[corvus::handler]
fn save_bisect_session(
    state: &AppState,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    name: Option<String>,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::save_and_pause(&git_cli(), &repo.path, bad_hashes, good_hashes, name)?)
}

#[corvus::handler]
fn save_bisect_result(
    state: &AppState,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    result_hash: String,
    result_message: Option<String>,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::save_result(&repo.path, bad_hashes, good_hashes, result_hash, result_message)?)
}

#[corvus::handler]
fn resume_bisect_session(state: &AppState, tab_id: String, session_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::resume_session(&git_cli(), &repo.path, &session_id)?)
}

#[corvus::handler]
fn rename_bisect_session(
    state: &AppState,
    tab_id: String,
    session_id: String,
    new_name: String,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::rename_session(&repo.path, &session_id, new_name)?)
}

#[corvus::handler]
fn delete_bisect_session(state: &AppState, tab_id: String, session_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::prelude::delete_session(&repo.path, &session_id)?)
}
