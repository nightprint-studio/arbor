//! `stash` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name** (reading the signature to generate the JSON-arg decode), so the
//! command is reached generically through the router. Behavior (locks held,
//! hooks fired, errors) is byte-identical — only the call path changed.
//!
//! Adding a command here = writing one annotated function. No explicit method
//! name, no arg-struct, no `match`, no central list.

use crate::error::AppError;
use crate::git::stash::{StashApplyResult, StashBlockingContent, StashEntry, StashRef};
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn list_stashes(state: &AppState, tab_id: String) -> Result<Vec<StashEntry>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::list_stashes(repo.inner_mut())
}

#[corvus::handler]
fn list_graph_stash_refs(state: &AppState, tab_id: String) -> Result<Vec<StashRef>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::collect_stash_refs(repo.inner_mut())
}

#[corvus::handler]
fn stash_save(
    state: &AppState,
    tab_id: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<StashEntry, AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };
    let entry = crate::git::stash::stash_save(&workdir, message.as_deref(), include_untracked)?;
    // `on_stash_push` is fired routing-independently by `corvus::post_hooks`
    // after the call returns (so it also fires when this method runs OOP).
    Ok(entry)
}

#[corvus::handler]
fn stash_apply(state: &AppState, tab_id: String, index: usize) -> Result<StashApplyResult, AppError> {
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        crate::git::stash::stash_apply(repo.inner_mut(), index)?
    };
    // `on_stash_pop` (drop:false, only when clean) is fired by
    // `corvus::post_hooks` after the call returns — OOP-safe.
    Ok(result)
}

#[corvus::handler]
fn stash_pop(state: &AppState, tab_id: String, index: usize) -> Result<StashApplyResult, AppError> {
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        crate::git::stash::stash_pop(repo.inner_mut(), index)?
    };
    // `on_stash_pop` (drop:true, only when clean) is fired by
    // `corvus::post_hooks` after the call returns — OOP-safe.
    Ok(result)
}

#[corvus::handler]
fn force_stash_apply(
    state: &AppState,
    tab_id: String,
    index: usize,
    files_to_delete: Vec<String>,
    files_to_keep: Vec<String>,
    drop_on_success: bool,
) -> Result<StashApplyResult, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::force_stash_apply(
        repo.inner_mut(), index, &files_to_delete, &files_to_keep, drop_on_success,
    )
}

#[corvus::handler]
fn abort_stash_apply(state: &AppState, tab_id: String) -> Result<(), AppError> {
    // Hold the lock across the snapshot+abort so a concurrent operation cannot
    // race with us and overwrite the snapshot's implicit refs.
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let workdir = repo
        .inner()
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();
    crate::git::stash::abort_stash_apply_with_snapshot(&workdir, Some(repo.inner()))
}

#[corvus::handler]
fn write_workdir_file(
    state: &AppState,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };
    let abs = workdir.join(&path);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("failed to create parent dirs for {path}: {e}")))?;
    }
    let bytes = crate::git::encoding::encode_for_disk(&content, encoding.as_deref());
    std::fs::write(&abs, &bytes)
        .map_err(|e| AppError::Other(format!("failed to write {path}: {e}")))?;
    Ok(())
}

#[corvus::handler]
fn get_stash_file_content(
    state: &AppState,
    tab_id: String,
    index: usize,
    path: String,
    encoding_override: Option<String>,
) -> Result<StashBlockingContent, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::stash::get_stash_file_content(repo.inner(), index, &path, encoding_override.as_deref())
}

#[corvus::handler]
fn stash_drop(state: &AppState, tab_id: String, index: usize) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::stash_drop(repo.inner_mut(), index)
}

#[corvus::handler]
fn stash_rename(
    state: &AppState,
    tab_id: String,
    index: usize,
    new_message: String,
) -> Result<StashEntry, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::stash_rename(repo.inner_mut(), index, &new_message)
}
