//! `submodule` domain — Tauri-free submodule listing + git operations.
//!
//! Lifted from the shell `crate::git::submodule`. Two couplings the crate
//! refuses get injected by the caller:
//!
//!   1. The git-binary invoker — an explicit [`GitCli`] passed in (no global
//!      `crate::git_cli::command()` state).
//!   2. Auth-arg resolution — keyring-backed, so it CANNOT live here. The
//!      caller injects a `resolve_auth: &(dyn Fn(&str) -> Vec<String> + Send +
//!      Sync)` that maps a single remote URL to the host-scoped `-c …` prefix
//!      pairs git should run with. The crate resolves submodule URLs from
//!      git2 (pure) and calls the resolver; the shell binds it to
//!      `crate::git_cli::http_auth_args_for_url` (keyring lookup stays
//!      shell-side).
//!
//! `Send + Sync` on the resolver keeps callers' futures `Send` if they ever
//! hold it across an `.await`.
//!
//! `list_submodules` (incl. ahead/behind from upstream) is pure git2.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

/// Resolver injected by the caller: maps a single remote URL to the
/// host-scoped auth `-c …` argv prefix (keyring-backed, hence shell-side).
/// `Send + Sync` so a holding future stays `Send` across an `.await`.
pub type AuthArgsResolver<'a> = &'a (dyn Fn(&str) -> Vec<String> + Send + Sync);

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
        .ok_or_else(|| GitError::Other("bare repository has no workdir".into()))?
        .to_path_buf();

    let mut out = Vec::new();

    for sub in repo.submodules().map_err(GitError::Git)? {
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

pub fn submodule_list_branches(repo: &Repository, sub_path: &str) -> Result<Vec<String>> {
    let p = abs_path(repo, sub_path)?;
    let inner = Repository::open(&p).map_err(GitError::Git)?;

    let mut set = std::collections::HashSet::new();

    // Local branches
    for b in inner.branches(Some(BranchType::Local)).map_err(GitError::Git)? {
        let (b, _) = b.map_err(GitError::Git)?;
        if let Ok(Some(name)) = b.name() {
            set.insert(name.to_string());
        }
    }

    // Remote branches — strip "origin/" prefix, skip /HEAD pseudo-refs
    for b in inner.branches(Some(BranchType::Remote)).map_err(GitError::Git)? {
        let (b, _) = b.map_err(GitError::Git)?;
        if let Ok(Some(name)) = b.name() {
            if name.ends_with("/HEAD") { continue; }
            let short = name.split_once('/').map(|x| x.1).unwrap_or(name);
            set.insert(short.to_string());
        }
    }

    let mut result: Vec<String> = set.into_iter().collect();
    result.sort();
    Ok(result)
}

// ---------------------------------------------------------------------------
// Submodule-level git operations (spawn git CLI with cwd = submodule path)
// ---------------------------------------------------------------------------

pub fn submodule_fetch(git: &GitCli, repo: &Repository, sub_path: &str, resolve_auth: AuthArgsResolver) -> Result<()> {
    let p = abs_path(repo, sub_path)?;
    let auth = submodule_auth_args(repo, sub_path, resolve_auth);
    git_run_with_prefix(git, &p, &auth, &["fetch"])
}

pub fn submodule_pull(git: &GitCli, repo: &Repository, sub_path: &str, resolve_auth: AuthArgsResolver) -> Result<String> {
    let p = abs_path(repo, sub_path)?;
    let auth = submodule_auth_args(repo, sub_path, resolve_auth);
    git_output_with_prefix(git, &p, &auth, &["pull"])
}

pub fn submodule_push(git: &GitCli, repo: &Repository, sub_path: &str, resolve_auth: AuthArgsResolver) -> Result<String> {
    let p = abs_path(repo, sub_path)?;
    let auth = submodule_auth_args(repo, sub_path, resolve_auth);
    git_output_with_prefix(git, &p, &auth, &["push"])
}

pub fn submodule_checkout(git: &GitCli, repo: &Repository, sub_path: &str, branch: &str) -> Result<()> {
    let p = abs_path(repo, sub_path)?;
    git_run(git, &p, &["checkout", branch])
}

// ---------------------------------------------------------------------------
// Parent-level submodule update commands (init + update)
// ---------------------------------------------------------------------------

/// Update all submodules (init + update, optionally recursive).
pub fn update_submodules(git: &GitCli, repo_path: &str, recursive: bool, resolve_auth: AuthArgsResolver) -> Result<()> {
    let mut args = vec!["submodule", "update", "--init"];
    if recursive { args.push("--recursive"); }
    let auth = repo_submodule_auth_args(repo_path, resolve_auth);
    git_run_str_with_prefix(git, repo_path, &auth, &args)
}

/// Update a single named submodule (init + update, optionally recursive).
pub fn update_submodule(git: &GitCli, repo_path: &str, name: &str, recursive: bool, resolve_auth: AuthArgsResolver) -> Result<()> {
    let auth = repo_submodule_auth_args(repo_path, resolve_auth);
    if recursive {
        git_run_str_with_prefix(git, repo_path, &auth,
            &["submodule", "update", "--init", "--recursive", "--", name])
    } else {
        git_run_str_with_prefix(git, repo_path, &auth,
            &["submodule", "update", "--init", "--", name])
    }
}

/// Resolve the URL of a single submodule by working-tree path and return
/// the auth args Arbor should prepend to its CLI invocation.  Empty when
/// the URL is SSH/file or the caller has no stored token for that host.
fn submodule_auth_args(repo: &Repository, sub_path: &str, resolve_auth: AuthArgsResolver) -> Vec<String> {
    let url = repo
        .find_submodule(sub_path)
        .ok()
        .and_then(|s| s.url().map(|u| u.to_string()))
        .unwrap_or_default();
    if url.is_empty() { return Vec::new(); }
    resolve_auth(&url)
}

/// Collect every submodule URL declared by the parent repo and return one
/// host-scoped auth `-c` pair per known host.  Used by `submodule update
/// --recursive` so each submodule (potentially on a different forge) gets
/// the right token without leaking it to others.
///
/// URL → auth-args resolution is keyring-backed, so it stays in the injected
/// `resolve_auth` closure (per single URL). We collect submodule URLs from
/// git2 here (pure) and dedupe the resulting argv pairs across hosts so a
/// shared host emits a single `-c` pair — matching the old shell-side
/// `http_auth_args_for_urls` host-dedup behaviour.
fn repo_submodule_auth_args(repo_path: &str, resolve_auth: AuthArgsResolver) -> Vec<String> {
    let Ok(repo) = Repository::open(repo_path) else { return Vec::new(); };
    let urls: Vec<String> = repo
        .submodules()
        .ok()
        .map(|subs| subs.iter().filter_map(|s| s.url().map(|u| u.to_string())).collect())
        .unwrap_or_default();
    if urls.is_empty() { return Vec::new(); }

    // Dedupe by the produced argv pairs: the per-URL resolver returns
    // host-scoped `-c <key>=<val>` entries, and two submodules on the same
    // host yield identical pairs — collapse them so git sees one config
    // override per host.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut args: Vec<String> = Vec::new();
    for url in &urls {
        for a in resolve_auth(url) {
            if seen.insert(a.clone()) {
                args.push(a);
            }
        }
    }
    args
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn abs_path(repo: &Repository, sub_path: &str) -> Result<PathBuf> {
    let workdir = repo.workdir()
        .ok_or_else(|| GitError::Other("bare repository has no workdir".into()))?;
    Ok(workdir.join(sub_path))
}

fn git_run(git: &GitCli, dir: &Path, args: &[&str]) -> Result<()> {
    let out = git.command()
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(GitError::Io)?;
    if !out.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(())
}

// Variants that accept a prefix argv (typically host-scoped `-c …` pairs
// from the injected auth resolver).  The prefix is inserted BEFORE the
// subcommand so git treats it as global config overrides.

fn git_run_with_prefix(git: &GitCli, dir: &Path, prefix: &[String], args: &[&str]) -> Result<()> {
    let out = git.command()
        .args(prefix)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(GitError::Io)?;
    if !out.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(())
}

fn git_output_with_prefix(git: &GitCli, dir: &Path, prefix: &[String], args: &[&str]) -> Result<String> {
    let out = git.command()
        .args(prefix)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(GitError::Io)?;
    if !out.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn git_run_str_with_prefix(git: &GitCli, dir: &str, prefix: &[String], args: &[&str]) -> Result<()> {
    let out = git.command()
        .args(prefix)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(GitError::Io)?;
    if !out.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&out.stderr).to_string()
        ));
    }
    Ok(())
}
