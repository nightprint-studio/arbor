//! `diff` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::diff`] (so the headless `corvus-be`
//! shares it). This module keeps the original shell-facing API — same
//! signatures, `AppError` results — so the in-process consumers (the diff IPC
//! handlers in `ipc/corvus/diff.rs`, the streaming diff loader, blame) are
//! untouched. The diff domain is pure libgit2 + the `git` binary: no recovery
//! snapshot, no credentials, no app-config; `context_lines` arrives as an
//! explicit parameter from the caller.
//!
//! When `corvus-be` serves diff out-of-process (next step), the backend will
//! call `corvus_git::diff` directly with its own `GitCli`.

use git2::Repository;

// Re-export the data types so existing `crate::git::diff::Diff*` /
// `crate::git::diff::BlameLine` / `crate::git::diff::EncodingOverrides` paths
// resolve unchanged for every in-process caller.
pub use corvus_git::diff::{
    BlameLine, DiffFile, DiffHunk, DiffLine, DiffStats, DiffStatus, EncodingOverrides, LineKind,
};

use crate::error::Result;

pub fn parse_diff_meta(diff: &git2::Diff) -> Vec<DiffFile> {
    corvus_git::diff::parse_diff_meta(diff)
}

pub fn parse_diff_one(
    repo: &Repository,
    diff: &git2::Diff,
    i: usize,
    overrides: Option<&EncodingOverrides>,
) -> Result<DiffFile> {
    Ok(corvus_git::diff::parse_diff_one(repo, diff, i, overrides)?)
}

pub fn parse_diff(
    repo: &Repository,
    diff: &git2::Diff,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    Ok(corvus_git::diff::parse_diff(repo, diff, overrides)?)
}

pub fn get_commit_diff_meta(
    repo: &Repository,
    oid_str: &str,
    algo: Option<&str>,
) -> Result<Vec<DiffFile>> {
    Ok(corvus_git::diff::get_commit_diff_meta(repo, oid_str, algo)?)
}

pub fn get_commit_file_diff(
    repo: &Repository,
    oid_str: &str,
    path: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<DiffFile> {
    Ok(corvus_git::diff::get_commit_file_diff(repo, oid_str, path, context_lines, algo, overrides)?)
}

pub fn get_commits_range_diff_meta(
    repo: &Repository,
    base_oid_str: &str,
    target_oid_str: &str,
    algo: Option<&str>,
) -> Result<Vec<DiffFile>> {
    Ok(corvus_git::diff::get_commits_range_diff_meta(repo, base_oid_str, target_oid_str, algo)?)
}

pub fn get_commits_range_file_diff(
    repo: &Repository,
    base_oid_str: &str,
    target_oid_str: &str,
    path: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<DiffFile> {
    Ok(corvus_git::diff::get_commits_range_file_diff(
        repo, base_oid_str, target_oid_str, path, context_lines, algo, overrides,
    )?)
}

pub fn get_commit_diff(
    repo: &Repository,
    oid_str: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    Ok(corvus_git::diff::get_commit_diff(repo, oid_str, context_lines, algo, overrides)?)
}

pub fn build_workdir_diff<'a>(
    repo: &'a Repository,
    staged: bool,
    context_lines: u32,
    algo: Option<&str>,
) -> Result<git2::Diff<'a>> {
    Ok(corvus_git::diff::build_workdir_diff(repo, staged, context_lines, algo)?)
}

pub fn get_workdir_diff(
    repo: &Repository,
    staged: bool,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    Ok(corvus_git::diff::get_workdir_diff(repo, staged, context_lines, algo, overrides)?)
}

pub fn get_branch_diff(
    repo: &Repository,
    from_ref: &str,
    to_ref: &str,
    context_lines: u32,
    algo: Option<&str>,
    overrides: Option<&EncodingOverrides>,
) -> Result<Vec<DiffFile>> {
    Ok(corvus_git::diff::get_branch_diff(repo, from_ref, to_ref, context_lines, algo, overrides)?)
}

pub fn get_file_at_commit(
    repo: &Repository,
    oid_str: &str,
    path: &str,
    encoding_override: Option<&str>,
) -> Result<String> {
    Ok(corvus_git::diff::get_file_at_commit(repo, oid_str, path, encoding_override)?)
}

pub fn get_file_blame(repo: &Repository, path: &str) -> Result<Vec<BlameLine>> {
    Ok(corvus_git::diff::get_file_blame(repo, path)?)
}
