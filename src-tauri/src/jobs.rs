use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Output line batching
// ---------------------------------------------------------------------------
// A child process generating verbose output (build tool, dev server, tomcat,
// …) can produce hundreds of lines per second.  Emitting one Tauri IPC event
// per line is wasteful in the steady state and pathological when the window
// is unfocused: WebView2 is power-throttled, so events queue in the IPC
// channel and are drained in a burst when focus returns — overwhelming the
// frontend's reactive state and freezing the UI for the duration of the
// drain.
//
// `LineBatcher` collects lines and emits them as `arbor://job-output-batch`
// in chunks bounded by either size (`BATCH_MAX_LINES`) or time
// (`BATCH_MAX_DURATION`).  Both stdout and stderr share a single batcher per
// job so flushes are ordered against each other.  A final `flush()` from the
// owning thread guarantees the tail of the stream reaches the frontend
// before `arbor://job-done`.
const BATCH_MAX_LINES:    usize    = 100;
const BATCH_MAX_DURATION: Duration = Duration::from_millis(50);

struct LineBatcher {
    app_handle: AppHandle,
    job_id:     String,
    inner:      Mutex<BatcherInner>,
}

struct BatcherInner {
    buf:      Vec<String>,
    first_at: Option<Instant>,
}

impl LineBatcher {
    fn new(app_handle: AppHandle, job_id: String) -> Arc<Self> {
        Arc::new(Self {
            app_handle,
            job_id,
            inner: Mutex::new(BatcherInner { buf: Vec::new(), first_at: None }),
        })
    }

    /// Append a line. Flush inline if the size or time threshold is reached.
    fn push(&self, line: String) {
        let to_emit = {
            let mut g = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            g.buf.push(line);
            if g.first_at.is_none() {
                g.first_at = Some(Instant::now());
            }
            let trigger = g.buf.len() >= BATCH_MAX_LINES
                || g.first_at.map_or(false, |t| t.elapsed() >= BATCH_MAX_DURATION);
            if trigger {
                g.first_at = None;
                Some(std::mem::take(&mut g.buf))
            } else {
                None
            }
        };
        if let Some(lines) = to_emit {
            self.emit_batch(lines);
        }
    }

    /// Drain whatever remains.  Called at end-of-stream so the trailing lines
    /// always reach the frontend.
    fn flush(&self) {
        let to_emit = {
            let mut g = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            g.first_at = None;
            if g.buf.is_empty() { None } else { Some(std::mem::take(&mut g.buf)) }
        };
        if let Some(lines) = to_emit {
            self.emit_batch(lines);
        }
    }

    fn emit_batch(&self, lines: Vec<String>) {
        let _ = self.app_handle.emit("arbor://job-output-batch", serde_json::json!({
            "job_id": &self.job_id,
            "lines":  lines,
        }));
    }
}

// ---------------------------------------------------------------------------
// Job types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum JobStatus {
    Running,
    Completed { exit_code: i32 },
    Failed { error: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobInfo {
    pub id: String,
    pub name: String,
    pub plugin_name: String,
    pub command: String,
    pub started_at: u64,
    pub status: JobStatus,
    /// Optional grouping category shown in the Jobs overlay (e.g. "Builds", "Services").
    pub category: Option<String>,
    /// When true the UI hides the cancel button — the job cannot be stopped by the user.
    #[serde(default)]
    pub non_cancellable: bool,
    /// When true the job is hidden from the default Jobs overlay / output panel
    /// listing and excluded from the status-bar running-count badge.  A "Show
    /// hidden" toggle on the Jobs panels reveals them as an escape hatch (e.g.
    /// when a domain-specific service-managed job becomes a zombie).
    #[serde(default)]
    pub hidden: bool,
    /// When true the job is internal (e.g. diff parsing, graph load) — it is
    /// purged from the registry a few seconds after completion so it does not
    /// clutter the Jobs overlay.  User-visible jobs (builds, plugin tasks) stay
    /// until the user clears them.
    #[serde(default)]
    pub is_system: bool,
    /// Unix timestamp at which the job entered a terminal state (Completed /
    /// Failed / Cancelled).  `None` while the job is still running.
    #[serde(default)]
    pub finished_at: Option<u64>,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct JobRegistry {
    jobs: HashMap<String, JobInfo>,
    /// Ring-buffer of output lines per job (max 2000 lines).
    outputs: HashMap<String, Vec<String>>,
    /// PID of the running process, if any — used for cancellation.
    pids: HashMap<String, u32>,
    counter: u64,
}

impl JobRegistry {
    pub fn new_id(&mut self) -> String {
        self.counter += 1;
        format!("job-{}", self.counter)
    }

    pub fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub fn register(&mut self, info: JobInfo) {
        self.outputs.insert(info.id.clone(), Vec::new());
        self.jobs.insert(info.id.clone(), info);
    }

    pub fn register_pid(&mut self, job_id: &str, pid: u32) {
        self.pids.insert(job_id.to_string(), pid);
    }

    pub fn append_output(&mut self, job_id: &str, line: String) {
        if let Some(lines) = self.outputs.get_mut(job_id) {
            lines.push(line);
            // Keep the last 2000 lines, dropping 200 at a time when full.
            if lines.len() > 2000 {
                lines.drain(0..200);
            }
        }
    }

    pub fn set_status(&mut self, job_id: &str, status: JobStatus) {
        let is_terminal = !matches!(status, JobStatus::Running);
        if let Some(info) = self.jobs.get_mut(job_id) {
            info.status = status;
            if is_terminal && info.finished_at.is_none() {
                info.finished_at = Some(Self::now_secs());
            }
        }
        // Clean up PID on terminal states.
        self.pids.remove(job_id);
        // Opportunistically purge old completed system jobs every time a
        // status transition occurs (keeps the overlay tidy with no timer).
        self.purge_stale_system_jobs();
    }

    /// Remove system jobs that finished more than `SYSTEM_JOB_TTL_SECS` ago.
    /// System jobs are short-lived internal tasks (diff parsing, graph loads)
    /// that should not accumulate in the Jobs overlay across a long session.
    pub fn purge_stale_system_jobs(&mut self) {
        const SYSTEM_JOB_TTL_SECS: u64 = 8;
        let now = Self::now_secs();
        let stale: Vec<String> = self.jobs.iter()
            .filter_map(|(id, info)| {
                if info.is_system {
                    if let Some(finished) = info.finished_at {
                        if now.saturating_sub(finished) >= SYSTEM_JOB_TTL_SECS {
                            return Some(id.clone());
                        }
                    }
                }
                None
            })
            .collect();
        for id in stale {
            self.jobs.remove(&id);
            self.outputs.remove(&id);
            self.pids.remove(&id);
        }
    }

    pub fn cancel(&mut self, job_id: &str) {
        if let Some(pid) = self.pids.remove(job_id) {
            kill_process(pid);
        }
        self.set_status(job_id, JobStatus::Cancelled);
    }

    /// Cancel all running jobs that belong to `plugin_name`.
    /// Pass `None` to cancel ALL running plugin jobs regardless of name.
    /// Jobs with `non_cancellable = true` (system jobs) are always skipped.
    pub fn cancel_by_plugin(&mut self, plugin_name: Option<&str>) {
        let ids: Vec<String> = self.jobs.values()
            .filter(|j| {
                j.status == JobStatus::Running
                    && !j.non_cancellable
                    && plugin_name.map_or(true, |p| j.plugin_name == p)
            })
            .map(|j| j.id.clone())
            .collect();
        for id in ids {
            self.cancel(&id);
        }
    }

    pub fn list(&self) -> Vec<JobInfo> {
        let mut v: Vec<JobInfo> = self.jobs.values().cloned().collect();
        // Most-recent first.
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v
    }

    /// Same as `list()` but also runs the stale-system-jobs purge first.
    /// Useful as a single entry point for the frontend listing command so the
    /// overlay never displays jobs that should have been cleaned up already.
    pub fn list_and_purge(&mut self) -> Vec<JobInfo> {
        self.purge_stale_system_jobs();
        self.list()
    }

    pub fn get_output(&self, job_id: &str) -> Vec<String> {
        self.outputs.get(job_id).cloned().unwrap_or_default()
    }

    pub fn running_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| j.status == JobStatus::Running)
            .count()
    }

    /// Remove a single job from the registry — only allowed for terminal jobs.
    /// Returns true when removed; false when missing or still running.
    pub fn dismiss(&mut self, job_id: &str) -> bool {
        let is_terminal = self.jobs.get(job_id)
            .map(|j| !matches!(j.status, JobStatus::Running))
            .unwrap_or(false);
        if !is_terminal { return false; }
        self.jobs.remove(job_id);
        self.outputs.remove(job_id);
        self.pids.remove(job_id);
        true
    }

    /// Remove every job in a terminal state. Returns the IDs that were
    /// removed so the caller can mirror the change in the frontend store.
    pub fn clear_finished(&mut self) -> Vec<String> {
        let to_remove: Vec<String> = self.jobs.values()
            .filter(|j| !matches!(j.status, JobStatus::Running))
            .map(|j| j.id.clone())
            .collect();
        for id in &to_remove {
            self.jobs.remove(id);
            self.outputs.remove(id);
            self.pids.remove(id);
        }
        to_remove
    }

    /// True when the job exists and is in Cancelled state.
    pub fn is_cancelled(&self, job_id: &str) -> bool {
        self.jobs
            .get(job_id)
            .map(|j| j.status == JobStatus::Cancelled)
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Process kill helpers
// ---------------------------------------------------------------------------

pub(crate) fn kill_process(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .no_window()
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}

// ---------------------------------------------------------------------------
// Background job spawner — called from the Lua `arbor.job.spawn` API.
// Runs in a detached thread; no Lua VM involved during execution.
// ---------------------------------------------------------------------------

pub struct JobSpawnRequest {
    pub job_id: String,
    #[allow(dead_code)]
    pub name: String,
    pub plugin_name: String,
    pub command: String,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    /// Action name fired on the plugin when the job finishes.
    /// The context JSON will contain: { job_id, success, exit_code }.
    pub on_done_action: Option<String>,
    /// Optional grouping category (e.g. "Builds", "Services").
    #[allow(dead_code)]
    pub category: Option<String>,
}

pub fn spawn_job(req: JobSpawnRequest, app_handle: tauri::AppHandle) {
    use std::io::BufRead;
    use std::process::Stdio;

    let job_id_for_err = req.job_id.clone();
    if let Err(e) = std::thread::Builder::new()
        .name(format!("arbor-job-{}", req.job_id))
        .spawn(move || {
            // ── Build the platform command ──────────────────────────────────
            #[cfg(target_os = "windows")]
            let mut cmd = {
                use std::os::windows::process::CommandExt;
                let mut c = std::process::Command::new("cmd");
                // Use raw_arg so Rust does NOT double-quote / escape the command
                // string. `Command::arg` would wrap it in outer quotes and escape
                // inner quotes with backslashes, which causes cmd /C to mangle the
                // path. raw_arg passes the bytes as-is to CreateProcessW.
                c.raw_arg("/C");
                c.raw_arg(&req.command);
                c
            };
            #[cfg(not(target_os = "windows"))]
            let mut cmd = {
                let mut c = std::process::Command::new("sh");
                c.arg("-c").arg(&req.command);
                c
            };

            cmd.no_window();

            if let Some(ref dir) = req.cwd {
                cmd.current_dir(dir);
            }
            for (k, v) in &req.env {
                cmd.env(k, v);
            }
            // Force ANSI color output for common toolchains (cargo, npm, go, etc.).
            // Programs disable colors when they detect a pipe instead of a TTY;
            // these env vars override that heuristic.
            // Only set if the plugin hasn't explicitly provided the variable.
            let color_defaults: &[(&str, &str)] = &[
                ("TERM",             "xterm-256color"),
                ("COLORTERM",        "truecolor"),
                ("CLICOLOR_FORCE",   "1"),
                ("FORCE_COLOR",      "1"),
                ("CARGO_TERM_COLOR", "always"),
            ];
            for (k, v) in color_defaults {
                if !req.env.iter().any(|(ek, _)| ek == k) {
                    cmd.env(k, v);
                }
            }
            cmd.stdin(Stdio::null())
               .stdout(Stdio::piped())
               .stderr(Stdio::piped());

            // ── Spawn ───────────────────────────────────────────────────────
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let err = e.to_string();
                    let state = app_handle.state::<crate::AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        jobs.set_status(&req.job_id, JobStatus::Failed { error: err.clone() });
                    };
                    let _ = app_handle.emit("arbor://job-done", serde_json::json!({
                        "job_id":    req.job_id,
                        "success":   false,
                        "exit_code": -1,
                        "error":     err,
                    }));
                    return;
                }
            };

            // Store PID for cancellation.
            let pid = child.id();
            {
                let state = app_handle.state::<crate::AppState>();
                if let Ok(mut jobs) = state.jobs.lock() {
                    jobs.register_pid(&req.job_id, pid);
                };
            }

            // stdout/stderr are always Some after a successful spawn with Stdio::piped().
            let stdout = child.stdout.take().expect("stdout pipe missing after spawn");
            let stderr = child.stderr.take().expect("stderr pipe missing after spawn");

            // Shared batcher — both reader threads coalesce into the same
            // `arbor://job-output-batch` stream so flushes are ordered against
            // each other and the frontend receives at most one IPC event per
            // 50 ms / 100 lines per job.
            let batcher = LineBatcher::new(app_handle.clone(), req.job_id.clone());

            // ── Stderr reader thread ────────────────────────────────────────
            let job_id_err = req.job_id.clone();
            let handle_err = app_handle.clone();
            let batcher_err = batcher.clone();
            let stderr_thread = std::thread::spawn(move || {
                for line in std::io::BufReader::new(stderr).lines().flatten() {
                    let annotated = format!("[stderr] {}", line);
                    {
                        let state = handle_err.state::<crate::AppState>();
                        if let Ok(mut jobs) = state.jobs.lock() {
                            jobs.append_output(&job_id_err, annotated.clone());
                        };
                    }
                    batcher_err.push(annotated);
                }
            });

            // ── Stdout reader (main job thread) ─────────────────────────────
            for line in std::io::BufReader::new(stdout).lines().flatten() {
                {
                    let state = app_handle.state::<crate::AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        // Check if cancelled before appending.
                        if let Some(info) = jobs.jobs.get(&req.job_id) {
                            if info.status == JobStatus::Cancelled {
                                break;
                            }
                        }
                        jobs.append_output(&req.job_id, line.clone());
                    };
                }
                batcher.push(line);
            }

            let _ = stderr_thread.join();
            // Final drain — make sure the tail of stdout/stderr reaches the
            // frontend before we emit `arbor://job-done`.
            batcher.flush();

            // ── Wait for process ────────────────────────────────────────────
            let exit_code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            let success = exit_code == 0;

            // Check if already cancelled before overwriting status.
            {
                let state = app_handle.state::<crate::AppState>();
                if let Ok(mut jobs) = state.jobs.lock() {
                    if let Some(info) = jobs.jobs.get(&req.job_id) {
                        if info.status == JobStatus::Cancelled {
                            // Process was killed externally — notify the frontend so the
                            // UI updates from "running" to "cancelled".
                            let _ = app_handle.emit("arbor://job-done", serde_json::json!({
                                "job_id":    &req.job_id,
                                "success":   false,
                                "exit_code": exit_code,
                                "cancelled": true,
                            }));
                            // Also fire the on_done_action so plugins can clean up
                            // (e.g. untrack a build, release a lock).
                            drop(jobs); // release lock before calling into Lua
                            if let Some(ref action) = req.on_done_action {
                                let ctx = serde_json::json!({
                                    "job_id":    &req.job_id,
                                    "success":   false,
                                    "exit_code": exit_code,
                                    "cancelled": true,
                                }).to_string();
                                let state = app_handle.state::<crate::AppState>();
                                if let Ok(host) = state.plugin_host.lock() {
                                    let _ = host.fire_hook_on(&req.plugin_name, action, &ctx);
                                };
                            }
                            return;
                        }
                    }
                    let status = if success {
                        JobStatus::Completed { exit_code }
                    } else {
                        JobStatus::Failed { error: format!("exit code {}", exit_code) }
                    };
                    jobs.set_status(&req.job_id, status);
                };
            }

            let _ = app_handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &req.job_id,
                "success":   success,
                "exit_code": exit_code,
                "cancelled": false,
            }));

            // ── Fire on_done_action hook ────────────────────────────────────
            if let Some(ref action) = req.on_done_action {
                let ctx = serde_json::json!({
                    "job_id":    &req.job_id,
                    "success":   success,
                    "exit_code": exit_code,
                }).to_string();
                let state = app_handle.state::<crate::AppState>();
                if let Ok(host) = state.plugin_host.lock() {
                    let _ = host.fire_hook_on(&req.plugin_name, action, &ctx);
                };
            }
        })
    {
        tracing::error!("failed to spawn job thread for '{}': {e}", job_id_for_err);
    }
}
