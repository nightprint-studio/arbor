use tauri::State;
use crate::AppState;
use crate::error::AppError;

/// Cancel a running job (kills the process if still alive, marks status = cancelled).
///
/// DEFERRED from the `platform` jobs migration: this signals a live child
/// process (`JobRegistry::cancel` → `kill_process(pid)`) and races with the
/// `arbor://job-done` emit from `spawn_job`'s monitor thread, so it stays a
/// shell command until the emit/signal seam pass.
#[tauri::command]
pub fn cancel_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<(), AppError> {
    let mut jobs = state.lock_jobs()?;
    jobs.cancel(&job_id);
    Ok(())
}
