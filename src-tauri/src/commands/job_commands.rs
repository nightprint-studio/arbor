use tauri::State;
use crate::AppState;
use crate::error::AppError;
use crate::jobs::JobInfo;

/// List all registered jobs (most-recent first).  Also purges stale system jobs
/// so internal short-lived tasks (diff parsing, graph loads) do not pile up.
#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Vec<JobInfo>, AppError> {
    let mut jobs = state.lock_jobs()?;
    Ok(jobs.list_and_purge())
}

/// Return the accumulated output lines for a specific job.
#[tauri::command]
pub fn get_job_output(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Vec<String>, AppError> {
    let jobs = state.lock_jobs()?;
    Ok(jobs.get_output(&job_id))
}

/// Cancel a running job (kills the process if still alive, marks status = cancelled).
#[tauri::command]
pub fn cancel_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<(), AppError> {
    // Cloud-storage transfer jobs run as in-process tokio tasks (no PID),
    // so the standard kill path is a no-op for them — flip the cooperative
    // cancellation flag here, then fall through. Tasks check the flag at
    // every chunk boundary and abort the next opendal read/write.
    if let Ok(map) = state.cloud_cancellations.lock() {
        if let Some(flag) = map.get(&job_id) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let mut jobs = state.lock_jobs()?;
    jobs.cancel(&job_id);
    Ok(())
}

/// Return the number of currently running jobs.
#[tauri::command]
pub fn running_job_count(state: State<'_, AppState>) -> Result<usize, AppError> {
    let jobs = state.lock_jobs()?;
    Ok(jobs.running_count())
}

/// Remove a finished job from the registry (no-op if it's still running).
/// Returns true when the job was actually removed.
#[tauri::command]
pub fn dismiss_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<bool, AppError> {
    let mut jobs = state.lock_jobs()?;
    Ok(jobs.dismiss(&job_id))
}

/// Remove every finished job. Returns the IDs that were dropped so the
/// frontend can prune its local mirror without a full re-list.
#[tauri::command]
pub fn clear_finished_jobs(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let mut jobs = state.lock_jobs()?;
    Ok(jobs.clear_finished())
}
