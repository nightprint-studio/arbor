//! `jobs` domain — read/registry queries over the shared `JobRegistry`.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[platform::handler(program = "platform")]` self-registers it under its own
//! function name. The registry itself is the pure, Tauri-free model in
//! [`arbor_feedback`] (re-exported as [`crate::jobs::JobInfo`]); these handlers
//! just take the `state.lock_jobs()` guard and delegate. Behavior (locks held,
//! purge semantics, errors) is byte-identical to the old commands.
//!
//! `cancel_job` is **not** here: it calls `JobRegistry::cancel`, which kills the
//! running child process (`kill_process(pid)`) — i.e. it signals a live process
//! and races with the `arbor://job-done` emit from `spawn_job`'s monitor
//! thread. It stays inline in the old command module for the later emit/signal
//! seam pass.
//!
//! No hooks fire in this domain.

use crate::error::AppError;
use crate::ipc::platform;
use crate::jobs::JobInfo;
use crate::AppState;

/// List all registered jobs (most-recent first). Also purges stale system jobs
/// so internal short-lived tasks (diff parsing, graph loads) do not pile up.
#[platform::handler(program = "platform")]
fn list_jobs(state: &AppState) -> Result<Vec<JobInfo>, AppError> {
    let mut jobs = state.lock_jobs()?;
    Ok(jobs.list_and_purge())
}

/// Return the accumulated output lines for a specific job.
#[platform::handler(program = "platform")]
fn get_job_output(state: &AppState, job_id: String) -> Result<Vec<String>, AppError> {
    let jobs = state.lock_jobs()?;
    Ok(jobs.get_output(&job_id))
}

/// Return the number of currently running jobs.
#[platform::handler(program = "platform")]
fn running_job_count(state: &AppState) -> Result<usize, AppError> {
    let jobs = state.lock_jobs()?;
    Ok(jobs.running_count())
}

/// Remove a finished job from the registry (no-op if it's still running).
/// Returns true when the job was actually removed.
#[platform::handler(program = "platform")]
fn dismiss_job(state: &AppState, job_id: String) -> Result<bool, AppError> {
    let mut jobs = state.lock_jobs()?;
    Ok(jobs.dismiss(&job_id))
}

/// Remove every finished job. Returns the IDs that were dropped so the
/// frontend can prune its local mirror without a full re-list.
#[platform::handler(program = "platform")]
fn clear_finished_jobs(state: &AppState) -> Result<Vec<String>, AppError> {
    let mut jobs = state.lock_jobs()?;
    Ok(jobs.clear_finished())
}
