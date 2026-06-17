//! `diff` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. Behavior
//! (locks held, errors, brief-lock-then-reopen shape) is byte-identical — only
//! the call path changed.
//!
//! The pure git work already lives in the reusable shell module
//! [`crate::git::diff`] (libgit2-based, no Tauri / no credentials), so these
//! handlers delegate to it directly — no `corvus-git` crate extraction needed.
//! The generic `rpc` command already wraps dispatch in `spawn_blocking`, so the
//! per-handler `tokio::task::spawn_blocking` of the old async commands is
//! dropped: each handler reopens the repo off the brief repo-lock and runs the
//! git work inline.
//!
//! No hooks fire in this domain.
//!
//! Two streaming commands are intentionally NOT migrated and remain inline in
//! `commands/diff_commands.rs`: `get_workdir_diff_stream` (takes `AppHandle`,
//! emits `arbor://diff-stream-*` events, registers a job) and
//! `get_file_blame_streaming` (drives a `tauri::ipc::Channel` progress stream).
//! A later emit/seam pass handles them.

use std::collections::HashMap;

use crate::error::AppError;
use crate::git::diff::{BlameLine, DiffFile};
use crate::ipc::corvus;
use crate::AppState;

/// Frontend supplies `encoding_overrides` as `{ [path]: "windows-1252" }`.
/// `None` means "no overrides — auto-detect every file" (default behaviour).
type Overrides = Option<HashMap<String, String>>;

#[corvus::handler]
fn get_commit_diff(
    state: &AppState,
    tab_id: String,
    oid: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commit_diff(
        &repo, &oid, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_commit_diff_meta(
    state: &AppState,
    tab_id: String,
    oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<crate::git::diff::DiffFile>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commit_diff_meta(&repo, &oid, diff_algo.as_deref())
}

#[corvus::handler]
fn get_commit_file_diff(
    state: &AppState,
    tab_id: String,
    oid: String,
    path: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<crate::git::diff::DiffFile, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commit_file_diff(
        &repo, &oid, &path, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_commits_range_diff_meta(
    state: &AppState,
    tab_id: String,
    base_oid: String,
    target_oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<crate::git::diff::DiffFile>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commits_range_diff_meta(&repo, &base_oid, &target_oid, diff_algo.as_deref())
}

#[corvus::handler]
fn get_commits_range_file_diff(
    state: &AppState,
    tab_id: String,
    base_oid: String,
    target_oid: String,
    path: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<crate::git::diff::DiffFile, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commits_range_file_diff(
        &repo, &base_oid, &target_oid, &path, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_workdir_diff(
    state: &AppState,
    tab_id: String,
    staged: bool,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_workdir_diff(
        &repo, staged, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_file_at_commit(
    state: &AppState,
    tab_id: String,
    oid: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<String, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_file_at_commit(&repo, &oid, &path, encoding_override.as_deref())
}

#[corvus::handler]
fn get_branch_diff(
    state: &AppState,
    tab_id: String,
    from_ref: String,
    to_ref: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_branch_diff(
        &repo, &from_ref, &to_ref, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_file_blame(
    state: &AppState,
    tab_id: String,
    path: String,
) -> Result<Vec<BlameLine>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_file_blame(&repo, &path)
}
