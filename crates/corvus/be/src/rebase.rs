//! `rebase` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::rebase`), but the context is [`CorvusState`]: the
//! repo path comes from the shell-pushed registration ([`crate::repo::repo_path`])
//! and the git program from [`CorvusState::git_program`]. The pure git work is
//! the shared [`corvus_git::rebase`] crate, so behavior + error strings are
//! identical to in-process.
//!
//! **Hooks fire here, in-process to this backend.** Two handlers fire
//! fire-and-forget hooks around the git call (`start_rebase` →
//! `on_rebase_start`, `rebase_abort` → `on_rebase_abort`). They fire inline via
//! [`CorvusState::fire_hook`] to the co-located plugin host, **after** the
//! repo-path scope is dropped — Lua hooks call git ops, so firing under a held
//! handle would risk deadlock; same lock-then-fire discipline and payload
//! (field-for-field) as the shell's in-process copy.
//!
//! No recovery snapshots are taken in this domain (the in-process copy takes
//! none either), so the shared `SnapshotPolicy::default()` gap does not apply.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{RebaseState, RebaseTodoEntry};
use serde_json::json;

use crate::repo::{git, open, repo_path};

#[arbor_rpc::handler]
fn get_rebase_todo(
    state: &CorvusState,
    tab_id: String,
    base: String,
) -> Result<Vec<RebaseTodoEntry>, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::rebase::get_rebase_todo(&git(state), &path, &base).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn start_rebase(
    state: &CorvusState,
    tab_id: String,
    base: String,
    todo: Vec<RebaseTodoEntry>,
) -> Result<(), String> {
    // Resolve the path, run the rebase, then fire the hook with the path scope
    // already dropped: a Lua git op in the hook must not run while we'd hold any
    // repo handle. Mirrors the shell's in-process brief-lock-then-fire shape.
    {
        let path = repo_path(state, &tab_id)?;
        corvus_git::rebase::start_interactive_rebase(&git(state), &path, &base, &todo)
            .map_err(|e| e.to_string())?;
    }
    // Fire `on_rebase_start` inline with first-hand data (action_count = todo.len()).
    state.fire_hook(
        "on_rebase_start",
        json!({ "tab_id": tab_id, "base": base, "action_count": todo.len() }),
    );
    Ok(())
}

#[arbor_rpc::handler]
fn rebase_continue(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::rebase::rebase_continue(&git(state), &path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn rebase_abort(state: &CorvusState, tab_id: String) -> Result<(), String> {
    // Resolve the path, run the abort, then fire the hook with the path scope
    // dropped — same lock-then-fire discipline as the in-process copy.
    {
        let path = repo_path(state, &tab_id)?;
        corvus_git::rebase::rebase_abort(&git(state), &path).map_err(|e| e.to_string())?;
    }
    // Fire `on_rebase_abort` inline with first-hand data.
    state.fire_hook("on_rebase_abort", json!({ "tab_id": tab_id }));
    Ok(())
}

#[arbor_rpc::handler]
fn rebase_skip(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::rebase::rebase_skip(&git(state), &path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_rebase_state(state: &CorvusState, tab_id: String) -> Result<RebaseState, String> {
    // Read the libgit2 repository state directly — same fields as in-process.
    let repo = open(state, &tab_id)?;
    let git_state = repo.state();
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
