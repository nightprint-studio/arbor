use std::collections::HashMap;
use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Clone helpers
// ---------------------------------------------------------------------------

/// List branch names available on a remote URL without cloning.
/// Uses the system `git` binary so that SSH keys and credential helpers work,
/// and prepends `git_cli::http_auth_args_for_url` so Arbor's stored OAuth/PAT
/// is also honoured for HTTPS URLs.
pub fn list_remote_branches(url: &str) -> Result<Vec<String>> {
    let output = crate::git_cli::command()
        .args(crate::git_cli::http_auth_args_for_url(url))
        .args(["ls-remote", "--heads", url])
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("git not found: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Other(stderr));
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

/// Clone a remote repository.  Uses the system `git` binary so SSH keys
/// (`~/.ssh`, ssh-agent) work out of the box.  For HTTPS URLs Arbor injects
/// its stored OAuth/PAT for the host as `http.<host>.extraHeader=…` when
/// available, taking precedence over the OS credential helper for that
/// specific host — when Arbor has no token for the host, the OS helper /
/// GCM / netrc fall-through still applies.  Returns the path where the
/// repository was cloned.
pub fn clone_repo(opts: &CloneOptions) -> Result<String> {
    let mut cmd = crate::git_cli::command();
    cmd.args(crate::git_cli::http_auth_args_for_url(&opts.url));
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

    let output = cmd.no_window().output()
        .map_err(|e| AppError::Other(format!("git not found: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Other(stderr));
    }

    Ok(opts.dest_path.clone())
}

// ---------------------------------------------------------------------------
// Background clone — used by the Lua `arbor.repo.clone` API.
// Streams progress lines as `arbor://job-output` events and registers the job
// in the shared JobRegistry so it shows up in the Jobs overlay and can be
// cancelled from the UI (or programmatically via `arbor.job.cancel`).
// ---------------------------------------------------------------------------

pub struct CloneJobRequest {
    pub job_id:             String,
    pub plugin_name:        String,
    pub url:                String,
    pub dest:               String,
    pub branch:             Option<String>,
    pub shallow:            bool,
    pub recurse_submodules: bool,
    /// Synthetic action name the plugin host fires when the job ends.
    /// Context JSON includes: { job_id, success, exit_code, cancelled, dest, url, error? }.
    pub on_done_action:     Option<String>,
}

pub fn spawn_clone_job(req: CloneJobRequest, app_handle: tauri::AppHandle) {
    use std::io::BufRead;
    use std::process::Stdio;
    use tauri::{Emitter, Manager};
    use crate::jobs::JobStatus;

    let job_id_for_err = req.job_id.clone();
    if let Err(e) = std::thread::Builder::new()
        .name(format!("arbor-clone-{}", req.job_id))
        .spawn(move || {
            // ── Build the argv — no shell wrapping, so URLs and paths are safe ──
            let mut cmd = crate::git_cli::command();
            // Inject Arbor's stored token for the remote host (HTTPS only).
            // Adds `-c http.extraHeader="Authorization: ..."` BEFORE the
            // subcommand so the clone authenticates without requiring the
            // OS-level credential helper to be set up.
            cmd.args(crate::git_cli::http_auth_args_for_url(&req.url));
            cmd.arg("clone").arg("--progress");

            if let Some(ref b) = req.branch {
                if !b.is_empty() {
                    cmd.args(["--branch", b]);
                }
            }
            if req.shallow {
                cmd.args(["--depth", "1"]);
            }
            if req.recurse_submodules {
                cmd.arg("--recurse-submodules");
            }
            cmd.arg("--").arg(&req.url).arg(&req.dest);

            cmd.no_window();
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

            // Force git to emit progress even when stderr is a pipe.
            cmd.env("GIT_PROGRESS_DELAY", "0");

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let err = e.to_string();
                    let state = app_handle.state::<crate::AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        jobs.set_status(&req.job_id, JobStatus::Failed { error: err.clone() });
                    };
                    let _ = app_handle.emit("arbor://job-done", serde_json::json!({
                        "job_id":    &req.job_id,
                        "success":   false,
                        "exit_code": -1,
                        "cancelled": false,
                        "error":     err,
                    }));
                    return;
                }
            };

            let pid = child.id();
            {
                let state = app_handle.state::<crate::AppState>();
                if let Ok(mut jobs) = state.jobs.lock() {
                    jobs.register_pid(&req.job_id, pid);
                };
            }

            let stdout = child.stdout.take().expect("stdout pipe missing after spawn");
            let stderr = child.stderr.take().expect("stderr pipe missing after spawn");

            // git clone writes progress to stderr; stdout is usually empty.
            let job_id_err = req.job_id.clone();
            let handle_err = app_handle.clone();
            let stderr_thread = std::thread::spawn(move || {
                for line in std::io::BufReader::new(stderr).lines().flatten() {
                    {
                        let state = handle_err.state::<crate::AppState>();
                        if let Ok(mut jobs) = state.jobs.lock() {
                            if jobs.is_cancelled(&job_id_err) { break; }
                            jobs.append_output(&job_id_err, line.clone());
                        };
                    }
                    let _ = handle_err.emit("arbor://job-output", serde_json::json!({
                        "job_id": &job_id_err,
                        "text":   line,
                    }));
                }
            });

            for line in std::io::BufReader::new(stdout).lines().flatten() {
                {
                    let state = app_handle.state::<crate::AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        if jobs.is_cancelled(&req.job_id) { break; }
                        jobs.append_output(&req.job_id, line.clone());
                    };
                }
                let _ = app_handle.emit("arbor://job-output", serde_json::json!({
                    "job_id": &req.job_id,
                    "text":   line,
                }));
            }

            let _ = stderr_thread.join();

            let exit_code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            let success   = exit_code == 0;

            let cancelled = {
                let state = app_handle.state::<crate::AppState>();
                let c = state.jobs.lock()
                    .map(|j| j.is_cancelled(&req.job_id))
                    .unwrap_or(false);
                if !c {
                    if let Ok(mut jobs) = state.jobs.lock() {
                        let status = if success {
                            JobStatus::Completed { exit_code }
                        } else {
                            JobStatus::Failed { error: format!("exit code {}", exit_code) }
                        };
                        jobs.set_status(&req.job_id, status);
                    };
                }
                c
            };

            let _ = app_handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &req.job_id,
                "success":   success && !cancelled,
                "exit_code": exit_code,
                "cancelled": cancelled,
                "dest":      &req.dest,
                "url":       &req.url,
            }));

            if let Some(ref action) = req.on_done_action {
                let ctx = serde_json::json!({
                    "job_id":    &req.job_id,
                    "success":   success && !cancelled,
                    "exit_code": exit_code,
                    "cancelled": cancelled,
                    "dest":      &req.dest,
                    "url":       &req.url,
                }).to_string();
                let state = app_handle.state::<crate::AppState>();
                if let Ok(host) = state.plugin_host.lock() {
                    let _ = host.fire_hook_on(&req.plugin_name, action, &ctx);
                };
            }
        })
    {
        tracing::error!("failed to spawn clone thread for '{}': {e}", job_id_for_err);
    }
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

// ---------------------------------------------------------------------------
// RepoManager — owns all open repositories, keyed by tab_id
// ---------------------------------------------------------------------------

pub struct RepoManager {
    repos:     HashMap<String, GitRepo>,
    /// Paths of repos that have been evicted from memory but can be re-opened
    /// transparently on next access (tab still open in UI, git2 handle freed).
    suspended: HashMap<String, String>,
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
            .ok_or_else(|| AppError::RepoNotOpen(tab_id.to_string()))
    }

    pub fn get_mut(&mut self, tab_id: &str) -> Result<&mut GitRepo> {
        self._ensure_open(tab_id)?;
        self.repos
            .get_mut(tab_id)
            .ok_or_else(|| AppError::RepoNotOpen(tab_id.to_string()))
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
