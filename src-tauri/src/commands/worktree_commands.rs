//! IDE-detection Tauri command.
//!
//! The worktree CRUD + IDE config/launch commands were migrated to broker
//! handlers in `ipc/corvus/worktree.rs`. `start_ide_detection` stays inline
//! because it takes an `AppHandle` and streams probe results to the frontend
//! (`arbor://job-*`, `arbor://ide-detection-done` events) — a later emit/seam
//! pass will move it.

use std::path::Path;
use tauri::{Manager, State};

use crate::error::AppError;
use crate::git::worktree::BUILTIN_IDES;
use crate::AppState;
use crate::process_ext::NoWindowExt;

/// Kick off IDE detection as a non-cancellable background job.
/// Each IDE is probed in a detached thread; results are broadcast via
/// `arbor://ide-detection-done` once all probes complete.
/// Returns the assigned job_id so callers can correlate events if needed.
#[tauri::command]
pub fn start_ide_detection(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, AppError> {
    use tauri::Emitter;
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};

    // Snapshot path overrides from config (don't hold the lock in the thread).
    let path_overrides = {
        let cfg = state.lock_config()?;
        cfg.ide.path_overrides.clone()
    };

    // Register the job.
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            "IDE Detection".to_string(),
            plugin_name:     "arbor".to_string(),
            command:         "detect IDEs".to_string(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".to_string()),
            non_cancellable: true,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    let _ = app_handle.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        "IDE Detection",
        "plugin_name": "arbor",
        "command":     "detect IDEs",
        "category":    "System",
    }));

    // Probe each IDE in a detached thread — never blocks the Tauri command thread.
    let jid   = job_id.clone();
    let handle = app_handle.clone();
    let _thread = std::thread::Builder::new()
        .name("arbor-ide-detection".into())
        .spawn(move || {
            use crate::git::worktree::DetectedIde;
            let mut results: Vec<DetectedIde> = Vec::with_capacity(BUILTIN_IDES.len());

            for ide in BUILTIN_IDES {
                // Resolve the command to probe.
                let cmd = match path_overrides.get(ide.id) {
                    Some(ov) if !ov.is_empty() => ov.clone(),
                    _                           => ide.cmd.to_string(),
                };

                // Probe: prefer explicit path check, then `which`/`where`.
                let (available, detected_path) = if Path::new(&cmd).exists() {
                    (true, Some(cmd.clone()))
                } else {
                    let found = probe_which(&cmd);
                    (found.is_some(), found)
                };

                let line = if available {
                    format!("✓  {} — {}", ide.name, detected_path.as_deref().unwrap_or(&cmd))
                } else {
                    format!("✗  {} — not found", ide.name)
                };

                // Append to ring-buffer.
                {
                    let s = handle.state::<AppState>();
                    if let Ok(mut jobs) = s.jobs.lock() {
                        jobs.append_output(&jid, line.clone());
                    };
                }
                let _ = handle.emit("arbor://job-output", serde_json::json!({
                    "job_id": &jid,
                    "text":   line,
                }));

                results.push(DetectedIde {
                    id:             ide.id.to_string(),
                    name:           ide.name.to_string(),
                    available,
                    detected_path,
                });
            }

            // Mark job complete.
            {
                let s = handle.state::<AppState>();
                if let Ok(mut jobs) = s.jobs.lock() {
                    jobs.set_status(&jid, JobStatus::Completed { exit_code: 0 });
                };
            }

            // Broadcast job-done + custom event carrying the results.
            let _ = handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &jid,
                "success":   true,
                "exit_code": 0,
            }));
            let _ = handle.emit("arbor://ide-detection-done", &results);
        });

    Ok(job_id)
}

/// Resolve a command via `which` (Unix) / `where` (Windows).
fn probe_which(cmd: &str) -> Option<String> {
    #[cfg(windows)]
    let prog = "where";
    #[cfg(not(windows))]
    let prog = "which";

    std::process::Command::new(prog)
        .arg(cmd)
        .no_window()
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        })
}
