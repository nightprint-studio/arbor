//! `repo` domain — repository handle, metadata, in-memory open-repo registry,
//! and clone / remote-listing, all Tauri-free.
//!
//! Lifted from the shell `crate::git::repo`; only the couplings the crate
//! refuses are swapped:
//!
//! * git invocation goes through an explicit [`GitCli`] passed in (no
//!   process-global program resolution).
//! * the HTTPS auth args that the shell prepends from its keyring
//!   (`http.<host>.extraHeader=…`) become an **injected resolver closure**:
//!   the crate has no keyring, so the shell binds it to
//!   `crate::git_cli::http_auth_args_for_url`. When the caller has nothing to
//!   inject it can pass a closure returning an empty `Vec`.
//!
//! NOT moved (stays shell-side): the background clone *job* (`spawn_clone_job`)
//! — it needs `AppHandle`/`AppState`/`JobStatus`/the plugin host to stream
//! progress and fire the done-hook. The shell wrapper keeps it and calls
//! [`clone_repo`] here for the actual clone work.
//!
//! `RepoManager::get`/`get_mut` surface a "not open" failure as
//! [`GitError::Other`] carrying the exact string the shell's
//! `AppError::RepoNotOpen` rendered (`Repository not open for tab '<id>'`), so
//! the serialized wire string is byte-identical to before the extraction.

use std::collections::HashMap;

use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

// ---------------------------------------------------------------------------
// Clone helpers
// ---------------------------------------------------------------------------

/// List branch names available on a remote URL without cloning.
///
/// Uses the system `git` binary (via the injected [`GitCli`]) so that SSH keys
/// and credential helpers work, and prepends the caller-resolved
/// `auth_args` — the shell binds this to `crate::git_cli::http_auth_args_for_url`
/// so Arbor's stored OAuth/PAT is honoured for HTTPS URLs. Keyring resolution
/// stays shell-side; this crate only sees the already-resolved argv prefix.
///
/// COLLISION NOTE: `branch::list_remote_branches(repo)` (libgit2, lists the
/// remote-tracking branches of an *open* repo) shares this name. They are
/// distinct functions in distinct modules; the prelude re-exports only one
/// flat, the other by module path.
pub fn list_remote_branches(git: &GitCli, url: &str, auth_args: &[String]) -> Result<Vec<String>> {
    let output = git
        .command()
        .args(auth_args)
        .args(["ls-remote", "--heads", url])
        .output()
        .map_err(|e| GitError::Other(format!("git not found: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::Other(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches = stdout
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .filter_map(|r| r.strip_prefix("refs/heads/"))
        .map(String::from)
        .collect();

    Ok(branches)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloneOptions {
    pub url:               String,
    pub dest_path:         String,
    #[serde(default)]
    pub branch:            Option<String>,
    #[serde(default)]
    pub shallow:           bool,
    #[serde(default)]
    pub recurse_submodules: bool,
}

/// Clone a remote repository. Uses the system `git` binary (via the injected
/// [`GitCli`]) so SSH keys (`~/.ssh`, ssh-agent) work out of the box. For HTTPS
/// URLs the caller-resolved `auth_args` inject Arbor's stored OAuth/PAT for the
/// host (taking precedence over the OS credential helper for that host); when
/// empty, the OS helper / GCM / netrc fall-through still applies. Returns the
/// path where the repository was cloned.
pub fn clone_repo(git: &GitCli, opts: &CloneOptions, auth_args: &[String]) -> Result<String> {
    let mut cmd = git.command();
    cmd.args(auth_args);
    cmd.arg("clone");

    if let Some(branch) = &opts.branch {
        if !branch.is_empty() {
            cmd.args(["--branch", branch]);
        }
    }

    if opts.shallow {
        cmd.args(["--depth", "1"]);
    }

    if opts.recurse_submodules {
        cmd.arg("--recurse-submodules");
    }

    cmd.arg("--").arg(&opts.url).arg(&opts.dest_path);

    let output = cmd
        .output()
        .map_err(|e| GitError::Other(format!("git not found: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::Other(stderr));
    }

    Ok(opts.dest_path.clone())
}

// ---------------------------------------------------------------------------
// GitRepo
// ---------------------------------------------------------------------------

/// Wraps `git2::Repository` with pre-computed metadata.
pub struct GitRepo {
    pub path: String,
    pub name: String,
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self> {
        let repo = Repository::open(path)?;
        let workdir = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_path_buf();

        let path_str = workdir.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
        let name = workdir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());

        Ok(Self { path: path_str, name, repo })
    }

    #[inline]
    pub fn inner(&self) -> &Repository {
        &self.repo
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut Repository {
        &mut self.repo
    }

    /// Resolved short name of HEAD (branch name or "(detached)").
    pub fn current_branch(&self) -> Option<String> {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
    }
}

// ---------------------------------------------------------------------------
// DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub tab_id: String,
    pub path: String,
    pub name: String,
    pub current_branch: Option<String>,
    pub is_bare: bool,
    pub is_empty: bool,
}

impl RepoInfo {
    /// Build repo metadata for a path **without** registering an open tab — the
    /// `tab_id` is left empty. `clone_repo` uses this: it clones to disk and
    /// returns the fresh repo's metadata, leaving the "open it as a tab" step to
    /// the frontend (which keys the tab by the workspace-registry id, not a
    /// throwaway clone-time id).
    pub fn for_path(path: &str) -> Result<Self> {
        let git_repo = GitRepo::open(path)?;
        Ok(Self {
            tab_id: String::new(),
            path: git_repo.path.clone(),
            name: git_repo.name.clone(),
            current_branch: git_repo.current_branch(),
            is_bare: git_repo.inner().is_bare(),
            is_empty: git_repo.inner().is_empty().unwrap_or(false),
        })
    }
}

// ---------------------------------------------------------------------------
// RepoManager — owns all open repositories, keyed by tab_id
// ---------------------------------------------------------------------------

pub struct RepoManager {
    repos:     HashMap<String, GitRepo>,
    /// Paths of repos that have been evicted from memory but can be re-opened
    /// transparently on next access (tab still open in UI, git2 handle freed).
    suspended: HashMap<String, String>,
}

impl Default for RepoManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RepoManager {
    pub fn new() -> Self {
        Self { repos: HashMap::new(), suspended: HashMap::new() }
    }

    /// Open a repository and register it for `tab_id`.
    pub fn open(&mut self, tab_id: String, path: &str) -> Result<RepoInfo> {
        self.suspended.remove(&tab_id); // clear any suspended entry for this tab
        let git_repo = GitRepo::open(path)?;
        let info = RepoInfo {
            tab_id: tab_id.clone(),
            path: git_repo.path.clone(),
            name: git_repo.name.clone(),
            current_branch: git_repo.current_branch(),
            is_bare: git_repo.inner().is_bare(),
            is_empty: git_repo.inner().is_empty().unwrap_or(false),
        };
        self.repos.insert(tab_id, git_repo);
        Ok(info)
    }

    /// Remove a repository from memory (tab closed).
    pub fn close(&mut self, tab_id: &str) {
        self.repos.remove(tab_id);
        self.suspended.remove(tab_id);
    }

    /// Drop the git2::Repository handle to free libgit2 internal caches while
    /// keeping the path so the repo can be transparently re-opened on next access.
    pub fn evict_repo(&mut self, tab_id: &str) {
        if let Some(git_repo) = self.repos.remove(tab_id) {
            self.suspended.insert(tab_id.to_string(), git_repo.path.clone());
        }
    }

    /// Drop every open git2::Repository handle. Mainly called by plugins that
    /// are about to mutate the filesystem of the active repo via the CLI (or
    /// clone it): libgit2 can hold packfiles memory-mapped which blocks other
    /// processes from renaming/deleting them on Windows, so releasing the
    /// handles before handing the repo over avoids ERROR_SHARING_VIOLATION /
    /// permission-denied failures. The repos are re-opened transparently on
    /// the next `get()` / `get_mut()`.
    pub fn evict_all(&mut self) {
        let keys: Vec<String> = self.repos.keys().cloned().collect();
        for k in keys { self.evict_repo(&k); }
    }

    pub fn get(&mut self, tab_id: &str) -> Result<&GitRepo> {
        self._ensure_open(tab_id)?;
        self.repos
            .get(tab_id)
            .ok_or_else(|| repo_not_open(tab_id))
    }

    pub fn get_mut(&mut self, tab_id: &str) -> Result<&mut GitRepo> {
        self._ensure_open(tab_id)?;
        self.repos
            .get_mut(tab_id)
            .ok_or_else(|| repo_not_open(tab_id))
    }

    /// Re-open a suspended repo if needed. No-op if already open.
    fn _ensure_open(&mut self, tab_id: &str) -> Result<()> {
        if self.repos.contains_key(tab_id) {
            return Ok(());
        }
        if let Some(path) = self.suspended.remove(tab_id) {
            match GitRepo::open(&path) {
                Ok(git_repo) => {
                    self.repos.insert(tab_id.to_string(), git_repo);
                }
                Err(e) => {
                    // Put back so next attempt can retry
                    self.suspended.insert(tab_id.to_string(), path);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Returns info for every open tab.
    pub fn all_info(&self) -> Vec<RepoInfo> {
        self.repos
            .values()
            .map(|r| RepoInfo {
                tab_id: String::new(), // caller fills if needed
                path: r.path.clone(),
                name: r.name.clone(),
                current_branch: r.current_branch(),
                is_bare: r.inner().is_bare(),
                is_empty: r.inner().is_empty().unwrap_or(false),
            })
            .collect()
    }

    /// Returns `(tab_id, path, name)` for every open (non-suspended) tab.
    /// Used by plugin-reload to re-fire `on_repo_open` for all active tabs
    /// without accidentally forcing a suspended repo back into memory.
    pub fn list_open(&self) -> Vec<(String, String, String)> {
        self.repos
            .iter()
            .map(|(tab_id, r)| (tab_id.clone(), r.path.clone(), r.name.clone()))
            .collect()
    }
}

/// The crate's `GitError` has no `RepoNotOpen` variant (the shell owns that
/// `AppError` case). Surface the *exact* string `AppError::RepoNotOpen` rendered
/// so the serialized wire payload is byte-identical after the move. No caller
/// matches on the variant — it is only ever observed as this string — so the
/// type collapse to `Other` is invisible end-to-end.
fn repo_not_open(tab_id: &str) -> GitError {
    GitError::Other(format!("Repository not open for tab '{tab_id}'"))
}
