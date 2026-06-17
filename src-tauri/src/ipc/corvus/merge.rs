//! `merge` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The pure
//! git work now lives in [`corvus_git::merge`]; this shell layer keeps only the
//! repo-lock plumbing and the workdir extraction (releasing the lock before any
//! subprocess), so behavior (locks held, subprocess shelling, errors) is
//! byte-identical.
//!
//! No hooks fire in this domain.

use corvus_git::merge::{ConflictContent, ConflictPresence, MergeOutcome, MergeStrategy};

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> corvus_git::prelude::GitCli {
    corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path)
}

#[corvus::handler]
fn get_conflict_content(
    state: &AppState,
    tab_id: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<ConflictContent, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::merge::get_conflict_content(repo.inner(), &path, encoding_override.as_deref())?)
}

#[corvus::handler]
fn get_conflict_presence(
    state: &AppState,
    tab_id: String,
) -> Result<Vec<ConflictPresence>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::merge::get_conflict_presence(repo.inner())?)
}

#[corvus::handler]
fn resolve_conflict(
    state: &AppState,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    Ok(corvus_git::merge::resolve_conflict(repo.inner_mut(), &path, &content, encoding.as_deref())?)
}

#[corvus::handler]
fn remove_conflict_file(
    state: &AppState,
    tab_id: String,
    path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    Ok(corvus_git::merge::remove_conflict_file(repo.inner_mut(), &path)?)
}

#[corvus::handler]
fn resolve_stash_conflict(
    state: &AppState,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    Ok(corvus_git::merge::resolve_stash_conflict(repo.inner_mut(), &path, &content, encoding.as_deref())?)
}

#[corvus::handler]
fn complete_merge(
    state: &AppState,
    tab_id: String,
    message: String,
) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    Ok(corvus_git::merge::complete_merge(repo.inner_mut(), &message)?)
}

#[corvus::handler]
fn abort_merge(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    }; // release the lock before spawning a subprocess
    Ok(corvus_git::merge::abort_merge(&git(), &workdir)?)
}

#[corvus::handler]
fn merge_branch(
    state: &AppState,
    tab_id: String,
    branch_name: String,
    strategy: Option<MergeStrategy>,
) -> Result<MergeOutcome, AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    }; // lock released here
    Ok(corvus_git::merge::merge_branch(&git(), &workdir, &branch_name, strategy.unwrap_or_default())?)
}

#[corvus::handler]
fn get_merge_message(state: &AppState, tab_id: String) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::merge::get_merge_message(repo.inner())?)
}
