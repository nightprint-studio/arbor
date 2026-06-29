//! Job system — shell glue.
//!
//! The pure, Tauri-free job model (`JobRegistry`, `JobInfo`, `JobStatus`,
//! `kill_process`) lives in the `arbor-feedback` crate and is re-exported here
//! so the ~20 `crate::jobs::*` call sites across the shell keep working
//! unchanged. What stays in this file is the part that genuinely needs the
//! Tauri shell: the output `LineBatcher` (emits IPC events through `AppHandle`)
//! and `spawn_job` (spawns OS processes, touches `AppState`, fires plugin
//! hooks).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use crate::process_ext::NoWindowExt;

// Re-export the window-agnostic job model from the shared crate. Call sites use
// `crate::jobs::{JobInfo, JobRegistry, JobStatus}` exactly as before.
pub use arbor_feedback::prelude::{kill_process, JobInfo, JobRegistry, JobSpec, JobStatus};

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
                || g.first_at.is_some_and(|t| t.elapsed() >= BATCH_MAX_DURATION);
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
                for line in std::io::BufReader::new(stderr).lines().map_while(std::result::Result::ok) {
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
            for line in std::io::BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
                {
                    let state = app_handle.state::<crate::AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        // Check if cancelled before appending.
                        if jobs.is_cancelled(&req.job_id) {
                            break;
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
                    if jobs.is_cancelled(&req.job_id) {
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
                                arbor_plugin_core::prelude::fire_on(&host, &req.plugin_name, action, &ctx);
                            };
                            // The `on_done` closure may live in a product backend's
                            // VM (e.g. `corvus-be`): a BE-spawned job registers it
                            // under the synthetic `__job_done_<id>__` action name in
                            // that BE's `__arbor_hooks__` and forwards that name as
                            // `on_done_action`. Replay it there by name too, or a
                            // BE plugin's cancel cleanup never runs.
                            crate::ipc::fire_plugin_hook_on_backends(&app_handle, &req.plugin_name, action, &ctx);
                        }
                        return;
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
                    arbor_plugin_core::prelude::fire_on(&host, &req.plugin_name, action, &ctx);
                };
                // The `on_done` closure may live in a product backend's VM (e.g.
                // `corvus-be`): a BE-spawned job registers it under the synthetic
                // `__job_done_<id>__` action name in that BE's `__arbor_hooks__`
                // and forwards that name as `on_done_action`. Replay it there by
                // name too, or a BE plugin's job-completion callback never runs.
                crate::ipc::fire_plugin_hook_on_backends(&app_handle, &req.plugin_name, action, &ctx);
            }
        })
    {
        tracing::error!("failed to spawn job thread for '{}': {e}", job_id_for_err);
    }
}
