//! `graph` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The commit-graph lane algorithm, single-commit detail, and the
//! repo-file-tree / last-touch scan moved into [`corvus_git::graph`] (so the
//! headless `corvus-be` shares it). This module keeps the original shell-facing
//! API — same signatures, `AppError` results — so the in-process consumers
//! (the graph IPC handlers in `ipc/corvus/graph.rs`, the SVG export job) are
//! untouched.
//!
//! The graph reads through `&Repository` only (no git program / recovery
//! snapshot), so there is nothing to inject here beyond mapping the crate error
//! to `AppError` (via `?`). The `stashes` field of [`GraphData`] is collected
//! shell-side in `ipc/corvus/graph.rs::get_graph` through
//! [`crate::git::stash::collect_stash_refs`], exactly as before — `load_graph*`
//! leave it empty.

use git2::Repository;

use crate::error::Result;

// Re-export the data types so existing `crate::git::graph::*` paths resolve.
pub use corvus_git::prelude::{CommitDetail, GraphData, RepoFileEntry};

/// Load a paginated slice of the full commit graph.
pub fn load_graph(repo: &Repository, offset: usize, limit: usize) -> Result<GraphData> {
    Ok(corvus_git::graph::load_graph(repo, offset, limit)?)
}

/// Load a paginated slice of the commit graph filtered to commits that touched `file_path`.
pub fn load_graph_for_file(repo: &Repository, file_path: &str, offset: usize, limit: usize) -> Result<GraphData> {
    Ok(corvus_git::graph::load_graph_for_file(repo, file_path, offset, limit)?)
}

pub fn get_commit_detail(repo: &Repository, oid_str: &str) -> Result<CommitDetail> {
    Ok(corvus_git::graph::get_commit_detail(repo, oid_str)?)
}

/// Return all paths tracked by the index. Very fast — no commit walking.
pub fn get_repo_files(repo: &Repository) -> Result<Vec<String>> {
    Ok(corvus_git::graph::get_repo_files(repo)?)
}

/// Return the most-recent commit that touched each of the given paths.
pub fn get_files_last_commit(repo: &Repository, paths: Vec<String>) -> Result<Vec<RepoFileEntry>> {
    Ok(corvus_git::graph::get_files_last_commit(repo, paths)?)
}

/// Return all files tracked by the index together with the most-recent commit
/// that touched each one.
pub fn get_repo_file_tree(repo: &Repository) -> Result<Vec<RepoFileEntry>> {
    Ok(corvus_git::graph::get_repo_file_tree(repo)?)
}
