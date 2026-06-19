//! `worktree` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::worktree`), but the context is [`CorvusState`]:
//! the repo path comes from the shell-pushed registration and the git program
//! from [`CorvusState::git_program`]. The pure worktree git work (enumeration,
//! add/remove, project-type detection) is the shared [`corvus_git::worktree`]
//! crate, so the matched worktrees, the result shape, and the error strings are
//! byte-identical to in-process.
//!
//! Read-only / process-only domain — **no hooks fire here**.
//!
//! **Stays shell-side (NOT authored here):** every method that reaches an
//! `AppState`-only resource — the global IDE config (`lock_config`), the
//! per-repo `.arbor/config.toml` IDE preference (`config::repo_config`), the
//! detached IDE launch (`open_in_ide` process-spawn), and the streaming IDE
//! detection (event sink + job registry). Those route in-process; the
//! SplitBroker handles the per-method split.

use std::path::Path;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{ProjectType, WorktreeInfo};

use crate::repo::{git, repo_path};

// ---------------------------------------------------------------------------
// List / Add / Remove
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn list_worktrees(state: &CorvusState, tab_id: String) -> Result<Vec<WorktreeInfo>, String> {
    let path = repo_path(state, &tab_id)?;
    let repo_path = Path::new(&path);
    // In-process passes the repo path as both `repo_path` and `current_path`
    // (the active tab's path) — replicate that so `is_current` matches.
    corvus_git::worktree::list_worktrees(&git(state), repo_path, repo_path)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn add_worktree(
    state: &CorvusState,
    tab_id: String,
    dest_path: String,
    branch: String,
    new_branch: Option<String>,
) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::worktree::add_worktree(
        &git(state),
        Path::new(&path),
        &dest_path,
        &branch,
        new_branch.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn remove_worktree(
    state: &CorvusState,
    tab_id: String,
    worktree_path: String,
) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::worktree::remove_worktree(&git(state), Path::new(&path), &worktree_path)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Detect project type (standalone, no repo required)
// ---------------------------------------------------------------------------

// `detect_project_type` takes no repo — the handler macro requires a context
// first arg, so we accept `&CorvusState` and ignore it. The decoded JSON args
// (`path`) are unchanged, so the FE call is byte-identical to in-process.
#[arbor_rpc::handler]
fn detect_project_type(_state: &CorvusState, path: String) -> Result<ProjectType, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {path}"));
    }
    Ok(corvus_git::worktree::detect_project_type(p))
}
