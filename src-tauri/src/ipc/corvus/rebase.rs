//! `rebase` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The pure
//! git work now lives in [`corvus_git::rebase`]; this shell layer keeps only the
//! repo-lock plumbing — so behavior (locks held, subprocess shelling, errors) is
//! byte-identical.
//!
//! Two of the original commands fired fire-and-forget hooks around the git call
//! (`start_rebase` → `on_rebase_start`, `rebase_abort` → `on_rebase_abort`).
//! Those hooks are NOT fired here; the broker re-fires them post-dispatch (see
//! the integration checklist / `post_hooks`).

use corvus_git::rebase::{RebaseState, RebaseTodoEntry};

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> corvus_git::prelude::GitCli {
    corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path)
}

#[corvus::handler]
fn get_rebase_todo(
    state: &AppState,
    tab_id: String,
    base: String,
) -> Result<Vec<RebaseTodoEntry>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::rebase::get_rebase_todo(&git(), &repo.path, &base)?)
}

#[corvus::handler]
fn start_rebase(
    state: &AppState,
    tab_id: String,
    base: String,
    todo: Vec<RebaseTodoEntry>,
) -> Result<(), AppError> {
    // NOTE: the original command fired `on_rebase_start` (with `action_count =
    // todo.len()`) after this block; that hook is now re-fired by the broker
    // post-dispatch, so it is intentionally omitted here.
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::rebase::start_interactive_rebase(&git(), &repo.path, &base, &todo)?)
}

#[corvus::handler]
fn rebase_continue(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::rebase::rebase_continue(&git(), &repo.path)?)
}

#[corvus::handler]
fn rebase_abort(state: &AppState, tab_id: String) -> Result<(), AppError> {
    // NOTE: the original command fired `on_rebase_abort` after this block; that
    // hook is now re-fired by the broker post-dispatch, so it is omitted here.
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::rebase::rebase_abort(&git(), &repo.path)?)
}

#[corvus::handler]
fn rebase_skip(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(corvus_git::rebase::rebase_skip(&git(), &repo.path)?)
}

#[corvus::handler]
fn get_rebase_state(state: &AppState, tab_id: String) -> Result<RebaseState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let r = repo.inner();
    let git_state = r.state();
    let in_progress = matches!(
        git_state,
        git2::RepositoryState::Rebase
            | git2::RepositoryState::RebaseInteractive
            | git2::RepositoryState::RebaseMerge
    );
    Ok(RebaseState {
        in_progress,
        current_step: 0,
        total_steps: 0,
        conflicted_files: Vec::new(),
    })
}
