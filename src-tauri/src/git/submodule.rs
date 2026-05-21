use std::path::{Path, PathBuf};
use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmoduleInfo {
    pub name: String,
    /// Relative path from parent repo root.
    pub path: String,
    /// Absolute path (for opening as a tab).
    pub abs_path: String,
    pub url: String,
    /// Short 7-char HEAD commit hash (empty string if uninitialised).
    pub head_hash: String,
    /// Current branch name; `None` when detached HEAD.
    pub branch: Option<String>,
    /// Commits the submodule is ahead of its remote tracking branch.
    pub ahead: u32,
    /// Commits the submodule is behind its remote tracking branch.
    pub behind: u32,
    /// Whether the submodule working directory has uncommitted changes.
    pub is_dirty: bool,
    /// Whether the submodule has been initialised and cloned.
    pub is_initialized: bool,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub fn list_submodules(repo: &Repository) -> Result<Vec<SubmoduleInfo>> {
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no workdir".into()))?
        .to_path_buf();

    let mut out = Vec::new();

    for sub in repo.submodules().map_err(AppError::Git)? {
        let name    = sub.name().unwrap_or("").to_string();
        let path    = sub.path().to_string_lossy().to_string();
        let url     = sub.url().unwrap_or("").to_string();
        let abs     = workdir.join(&path);
        let abs_path = abs.to_string_lossy().to_string();

        match sub.open() {
            Err(_) => {
                // Submodule directory missing or not yet cloned.
                let head_hash = sub.head_id()
                    .map(|id| short7(&id.to_string()))
                    .unwrap_or_default();

                out.push(SubmoduleInfo {
                    name,
                    path,
                    abs_path,
                    url,
                    head_hash,
                    branch: None,
                    ahead: 0,
                    behind: 0,
                    is_dirty: false,
                    is_initialized: false,
                });
            }
            Ok(inner) => {
                let head_hash = inner.head()
                    .ok()
                    .and_then(|h| h.target())
                    .map(|id| short7(&id.to_string()))
                    .unwrap_or_default();

                let branch = if inner.head_detached().unwrap_or(true) {
                    None
                } else {
                    inner.head()
                        .ok()
                        .and_then(|h| h.shorthand().map(String::from))
                };

                let (ahead, behind) = branch.as_deref()
                    .map(|b| ahead_behind_from_upstream(&inner, b))
                    .unwrap_or((0, 0));

                let is_dirty = inner.statuses(None)
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);

                out.push(SubmoduleInfo {
                    name,
                    path,
                    abs_path,
                    url,
                    head_hash,
                    branch,
                    ahead,
                    behind,
                    is_dirty,
                    is_initialized: true,
                });
            }
        }
    }

    Ok(out)
}

/// Returns (ahead, behind) of the named local branch vs its upstream.
fn ahead_behind_from_upstream(repo: &Repository, branch_name: &str) -> (u32, u32) {
    let Ok(branch)   = repo.find_branch(branch_name, BranchType::Local) else { return (0, 0) };
    let Ok(upstream) = branch.upstream() else { return (0, 0) };
    let Some(local_oid) = branch.get().target()    else { return (0, 0) };
    let Some(up_oid)    = upstream.get().target()  else { return (0, 0) };
    repo.graph_ahead_behind(local_oid, up_oid)
        .map(|(a, b)| (a as u32, b as u32))
        .unwrap_or((0, 0))
}

fn short7(s: &str) -> String {
    s.chars().take(7).collect()
}

// ---------------------------------------------------------------------------
// Submodule-level git operations (spawn git CLI with cwd = submodule path)
// ---------------------------------------------------------------------------

pub fn submodule_fetch(repo: &Repository, sub_path: &str) -> Result<()> {
    let p = abs_path(repo, sub_path)?;
    let auth = submodule_auth_args(repo, sub_path);
    git_run_with_prefix(&p, &auth, &["fetch"])
}

pub fn submodule_pull(repo: &Repository, sub_path: &str) -> Result<String> {
    let p = abs_path(repo, sub_path)?;
    let auth = submodule_auth_args(repo, sub_path);
    git_output_with_prefix(&p, &auth, &["pull"])
}

pub fn submodule_push(repo: &Repository, sub_path: &str) -> Result<String> {
    let p = abs_path(repo, sub_path)?;
    let auth = submodule_auth_args(repo, sub_path);
    git_output_with_prefix(&p, &auth, &["push"])
}

pub fn submodule_checkout(repo: &Repository, sub_path: &str, branch: &str) -> Result<()> {
    let p = abs_path(repo, sub_path)?;
    git_run(&p, &["checkout", branch])
}

pub fn submodule_list_branches(repo: &Repository, sub_path: &str) -> Result<Vec<String>> {
    let p = abs_path(repo, sub_path)?;
    let inner = Repository::open(&p).map_err(AppError::Git)?;

    let mut set = std::collections::HashSet::new();

    // Local branches
    for b in inner.branches(Some(BranchType::Local)).map_err(AppError::Git)? {
        let (b, _) = b.map_err(AppError::Git)?;
        if let Ok(Some(name)) = b.name() {
            set.insert(name.to_string());
        }
    }

    // Remote branches — strip "origin/" prefix, skip /HEAD pseudo-refs
    for b in inner.branches(Some(BranchType::Remote)).map_err(AppError::Git)? {
        let (b, _) = b.map_err(AppError::Git)?;
        if let Ok(Some(name)) = b.name() {
            if name.ends_with("/HEAD") { continue; }
            let short = name.splitn(2, '/').nth(1).unwrap_or(name);
            set.insert(short.to_string());
        }
    }

    let mut result: Vec<String> = set.into_iter().collect();
    result.sort();
    Ok(result)
}

// ---------------------------------------------------------------------------
// Parent-level submodule update commands (init + update)
// ---------------------------------------------------------------------------

/// Update all submodules (init + update, optionally recursive).
pub fn update_submodules(repo_path: &str, recursive: bool) -> Result<()> {
    let mut args = vec!["submodule", "update", "--init"];
    if recursive { args.push("--recursive"); }
    let auth = repo_submodule_auth_args(repo_path);
    git_run_str_with_prefix(repo_path, &auth, &args)
}

/// Update a single named submodule (init + update, optionally recursive).
pub fn update_submodule(repo_path: &str, name: &str, recursive: bool) -> Result<()> {
    let auth = repo_submodule_auth_args(repo_path);
    if recursive {
        git_run_str_with_prefix(repo_path, &auth,
            &["submodule", "update", "--init", "--recursive", "--", name])
    } else {
        git_run_str_with_prefix(repo_path, &auth,
            &["submodule", "update", "--init", "--", name])
    }
}

/// Resolve the URL of a single submodule by working-tree path and return
/// the auth args Arbor should prepend to its CLI invocation.  Empty when
/// the URL is SSH/file or Arbor has no stored token for that host.
fn submodule_auth_args(repo: &Repository, sub_path: &str) -> Vec<String> {
    let url = repo
        .find_submodule(sub_path)
        .ok()
        .and_then(|s| s.url().map(|u| u.to_string()))
        .unwrap_or_default();
    if url.is_empty() { return Vec::new(); }
    crate::git_cli::http_auth_args_for_url(&url)
}

/// Collect every submodule URL declared by the parent repo and return one
/// host-scoped auth `-c` pair per known host.  Used by `submodule update
/// --recursive` so each submodule (potentially on a different forge) gets
/// the right token without leaking it to others.
fn repo_submodule_auth_args(repo_path: &str) -> Vec<String> {
    let Ok(repo) = Repository::open(repo_path) else { return Vec::new(); };
    let urls: Vec<String> = repo
        .submodules()
        .ok()
        .map(|subs| subs.iter().filter_map(|s| s.url().map(|u| u.to_string())).collect())
        .unwrap_or_default();
    if urls.is_empty() { return Vec::new(); }
    crate::git_cli::http_auth_args_for_urls(&urls)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn abs_path(repo: &Repository, sub_path: &str) -> Result<PathBuf> {
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no workdir".into()))?;
    Ok(workdir.join(sub_path))
}

fn git_run(dir: &Path, args: &[&str]) -> Result<()> {
    let out = crate::git_cli::command()
        .args(args)
        .current_dir(dir)
        .no_window()
        .output()
        .map_err(AppError::Io)?;
    if !out.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(())
}

// Variants that accept a prefix argv (typically host-scoped `-c …` pairs
// from `git_cli::http_auth_args_for_url(s)`).  The prefix is inserted
// BEFORE the subcommand so git treats it as global config overrides.

fn git_run_with_prefix(dir: &Path, prefix: &[String], args: &[&str]) -> Result<()> {
    let out = crate::git_cli::command()
        .args(prefix)
        .args(args)
        .current_dir(dir)
        .no_window()
        .output()
        .map_err(AppError::Io)?;
    if !out.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(())
}

fn git_output_with_prefix(dir: &Path, prefix: &[String], args: &[&str]) -> Result<String> {
    let out = crate::git_cli::command()
        .args(prefix)
        .args(args)
        .current_dir(dir)
        .no_window()
        .output()
        .map_err(AppError::Io)?;
    if !out.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn git_run_str_with_prefix(dir: &str, prefix: &[String], args: &[&str]) -> Result<()> {
    let out = crate::git_cli::command()
        .args(prefix)
        .args(args)
        .current_dir(dir)
        .no_window()
        .output()
        .map_err(AppError::Io)?;
    if !out.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(())
}
