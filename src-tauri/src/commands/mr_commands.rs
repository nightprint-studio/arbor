use tauri::State;

use crate::AppState;
use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Start MR conflict resolution
// ---------------------------------------------------------------------------
//
// DEFERRED from the Model-D `mr` migration: this is sync and streams progress
// through the `AppHandle` (custom events + Jobs registry), so it stays inline
// as a `#[tauri::command]` until the streaming/Channel seam lands. Everything
// else in the `mr` domain now lives in `ipc/corvus/mr.rs`.

/// Prepare the local workspace to resolve a pull/merge-request conflict.
///
/// This command spawns a background job (visible in the JobsOverlay) that runs
/// the multi-step prep flow without blocking the Tauri runtime.  Returns the
/// `job_id` immediately.  Progress is reported via two custom events:
///
/// - `arbor://mr-conflict-progress` — `{ job_id, phase, phase_index,
///   phase_total, label, detail? }`.  Drives the ProgressStepper widget.
/// - `arbor://mr-conflict-done`     — `{ job_id, status: "clean" |
///   "conflicts" | "error", error? }`.  Triggers the success / open-resolver /
///   error path on the frontend.
///
/// The job also emits the standard `arbor://job-started`, `arbor://job-output`
/// and `arbor://job-done` events so per-line stdout/stderr appears in the
/// Job Output panel.
#[tauri::command]
pub fn mr_start_conflict_resolution(
    state:         State<'_, AppState>,
    app_handle:    tauri::AppHandle,
    tab_id:        String,
    source_branch: String,
    target_branch: String,
) -> Result<String> {
    use tauri::{Emitter, Manager};
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};
    use crate::git::merge::{
        prepare_mr_conflict_resolution, MrPrepEvent, MrPrepOutcome, MrPrepPhase,
    };

    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };

    let name = format!("Resolve conflicts: {source_branch} ← {target_branch}");
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            name.clone(),
            plugin_name:     "arbor".to_string(),
            command:         format!("git fetch + checkout {source_branch} + merge origin/{target_branch}"),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("Merge".to_string()),
            non_cancellable: true,
            is_system:       false,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    let _ = app_handle.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &name,
        "plugin_name": "arbor",
        "command":     format!("git fetch + checkout {source_branch} + merge origin/{target_branch}"),
        "category":    "Merge",
    }));

    let jid    = job_id.clone();
    let handle = app_handle.clone();
    let _ = std::thread::Builder::new()
        .name(format!("arbor-mr-conflict-{}", jid))
        .spawn(move || {
            let result = prepare_mr_conflict_resolution(
                &workdir,
                &source_branch,
                &target_branch,
                |evt| match evt {
                    MrPrepEvent::PhaseStart { phase, detail } => {
                        let _ = handle.emit("arbor://mr-conflict-progress", serde_json::json!({
                            "job_id":      &jid,
                            "phase":       phase.key(),
                            "phase_index": phase.index(),
                            "phase_total": MrPrepPhase::TOTAL,
                            "label":       phase.label(),
                            "detail":      detail,
                        }));
                        let header = match &detail {
                            Some(d) => format!("── {} ({})", phase.label(), d),
                            None    => format!("── {}", phase.label()),
                        };
                        if let Ok(mut jobs) = handle.state::<crate::AppState>().jobs.lock() {
                            jobs.append_output(&jid, header.clone());
                        }
                        let _ = handle.emit("arbor://job-output", serde_json::json!({
                            "job_id": &jid, "text": header,
                        }));
                    }
                    MrPrepEvent::Output { phase: _, line } => {
                        if let Ok(mut jobs) = handle.state::<crate::AppState>().jobs.lock() {
                            jobs.append_output(&jid, line.to_string());
                        }
                        let _ = handle.emit("arbor://job-output", serde_json::json!({
                            "job_id": &jid, "text": line,
                        }));
                    }
                },
            );

            let (status_payload, outcome_label, error_msg) = match &result {
                Ok(MrPrepOutcome::Clean)     => (Ok(0i32), "clean",     None),
                Ok(MrPrepOutcome::Conflicts) => (Ok(0i32), "conflicts", None),
                Err(e)                       => (Err(()), "error",      Some(e.to_string())),
            };

            if let Ok(mut jobs) = handle.state::<crate::AppState>().jobs.lock() {
                let s = match status_payload {
                    Ok(c)  => JobStatus::Completed { exit_code: c },
                    Err(_) => JobStatus::Failed { error: error_msg.clone().unwrap_or_default() },
                };
                jobs.set_status(&jid, s);
            }

            let _ = handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &jid,
                "success":   matches!(status_payload, Ok(_)),
                "exit_code": status_payload.unwrap_or(-1),
            }));

            let _ = handle.emit("arbor://mr-conflict-done", serde_json::json!({
                "job_id": &jid,
                "status": outcome_label,
                "error":  error_msg,
            }));
        });

    Ok(job_id)
}
