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
