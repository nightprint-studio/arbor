//! `repo` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The repository handle, metadata DTOs, the in-memory open-repo registry
//! (`RepoManager`), and the clone / remote-listing helpers moved into
//! [`corvus_git::repo`] (so the headless `corvus-be` shares them). This module
//! keeps the original shell-facing API — same `crate::git::repo::*` paths — so
//! the in-process consumers (the repo IPC handlers, every `mgr.get(&tab_id)?`
//! call across the corvus IPC layer, linked-worktree orchestration, clone flow)
//! are untouched. It injects the shell's resolved git program (`GitCli`) and the
//! keyring-backed HTTPS auth args (which stay shell-side, in
//! `crate::git_cli::http_auth_args_for_url`).
//!
//! Re-exported types (`GitRepo`, `RepoInfo`, `RepoManager`, `CloneOptions`)
//! return the crate's `GitError`; the existing call sites all `?`-propagate, and
//! `impl From<GitError> for AppError` bridges them, so the wire string is
//! unchanged.
//!
//! NOT moved (stays shell-side): [`spawn_clone_job`] + [`CloneJobRequest`] — the
//! background clone *job* needs `AppHandle`/`AppState`/`JobStatus`/the plugin
//! host to stream progress events and fire the synthetic done-hook. It calls
//! [`corvus_git::repo::clone_repo`]-equivalent argv directly here because it must
//! interleave per-line progress streaming with cancellation checks the crate
//! has no notion of; the one-shot [`clone_repo`] wrapper below is what the
//! synchronous callers use.

use crate::error::Result;

// Re-export the data types + the open-repo registry so existing
// `crate::git::repo::*` paths resolve unchanged.
pub use corvus_git::prelude::{CloneOptions, GitRepo, RepoInfo, RepoManager};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> corvus_git::prelude::GitCli {
    corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// List branch names available on a remote URL without cloning.
pub fn list_remote_branches(url: &str) -> Result<Vec<String>> {
    // Keyring-backed HTTPS auth args stay shell-side; resolve them here and hand
    // the crate the already-built argv prefix.
    let auth = crate::git_cli::http_auth_args_for_url(url);
    Ok(corvus_git::repo::list_remote_branches(&git(), url, &auth)?)
}

/// Clone a remote repository. Returns the path where it was cloned.
pub fn clone_repo(opts: &CloneOptions) -> Result<String> {
    let auth = crate::git_cli::http_auth_args_for_url(&opts.url);
    Ok(corvus_git::repo::clone_repo(&git(), opts, &auth)?)
}

// ---------------------------------------------------------------------------
// Background clone — used by the Lua `arbor.repo.clone` API.
// Streams progress lines as `arbor://job-output` events and registers the job
// in the shared JobRegistry so it shows up in the Jobs overlay and can be
// cancelled from the UI (or programmatically via `arbor.job.cancel`).
//
// Stays shell-side: it is welded to AppHandle / AppState / JobStatus / the
// plugin host, none of which the Tauri-free crate may touch.
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
                    arbor_plugin_core::prelude::fire_on(&host, &req.plugin_name, action, &ctx);
                };
            }
        })
    {
        tracing::error!("failed to spawn clone thread for '{}': {e}", job_id_for_err);
    }
}
