//! `JobHandle` — drives a job in the shell's `JobRegistry` over the reverse channel
//! (ADR-3). bennu-be keeps **no** local registry: the shell's registry is the single
//! source of job state. A handler (here: the whole-project analysis warm-up) registers
//! a job — the shell mints the id — then reports terminal state through these methods.
//! The user-facing `arbor://job-*` events are emitted separately by the caller via the
//! [`BennuState`](bennu_core::prelude::BennuState) event sink (re-emitted by the shell);
//! this type carries only the *registry* state across the boundary, never the events.
//!
//! A near-verbatim twin of `corvus-be`'s `JobHandle` (same reverse-channel contract).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::HostCaller;
use bennu_core::prelude::BennuState;
use serde_json::json;

/// What to do when the person cancels a job from the Jobs panel, by job id.
///
/// The shell's registry only knows a job as a row: cancelling it marks the row, and kills a child process if there is
/// one — which a backend task never is. Without this the row said *cancelled* while the work went on, holding whatever
/// it held. The front end tells the backend which job was cancelled ([`bennu_cancel_job`]) and the task that owns it
/// says what stopping means for it.
type Action = Arc<dyn Fn() + Send + Sync>;

fn actions() -> &'static Mutex<HashMap<String, Action>> {
    static ACTIONS: OnceLock<Mutex<HashMap<String, Action>>> = OnceLock::new();
    ACTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A live job in the shell registry, addressed by the id the shell assigned.
pub struct JobHandle {
    host: Arc<dyn HostCaller>,
    pub id: String,
    /// Set when the person cancelled the job — so the task can tell a cancelled run from a finished one.
    cancelled: Arc<AtomicBool>,
}

impl JobHandle {
    /// Register `spec` in the shell's registry and return a handle. The shell assigns the
    /// id and stamps the start time, and the job enters `Running`.
    pub fn register(host: Arc<dyn HostCaller>, spec: JobSpec) -> Result<JobHandle, String> {
        let value = serde_json::to_value(spec).map_err(|e| e.to_string())?;
        let id = host.call("__job_register", value)?;
        let id: String = serde_json::from_value(id).map_err(|e| e.to_string())?;
        Ok(JobHandle { host, id, cancelled: Arc::new(AtomicBool::new(false)) })
    }

    /// Run `action` when the person cancels this job. The job must have been registered cancellable.
    pub fn on_cancel(&self, action: impl Fn() + Send + Sync + 'static) {
        let flag = Arc::clone(&self.cancelled);
        let action: Action = Arc::new(move || {
            flag.store(true, Ordering::Release);
            action();
        });
        actions().lock().unwrap_or_else(|p| p.into_inner()).insert(self.id.clone(), action);
    }

    /// Whether the person cancelled this job.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Append an output line to the job's buffer. Registry-only — the caller is responsible
    /// for emitting `arbor://job-output-batch` for the live stream, which is what the panel
    /// actually renders while the job runs; this is what a job opened *after* the fact still
    /// has to show.
    pub fn append(&self, line: &str) {
        let _ = self.host.call("__job_append", json!({ "job_id": self.id, "line": line }));
    }

    /// Set the job's status (terminal or running). Registry-only — the caller is
    /// responsible for emitting `arbor://job-done` on a terminal status.
    pub fn set_status(&self, status: JobStatus) {
        let _ = self
            .host
            .call("__job_set_status", json!({ "job_id": self.id, "status": status }));
    }
}

impl Drop for JobHandle {
    /// A finished job has nothing left to cancel.
    fn drop(&mut self) {
        actions().lock().unwrap_or_else(|p| p.into_inner()).remove(&self.id);
    }
}

/// The person cancelled job `job_id` in the Jobs panel: stop what it stands for. `false` when no task here owns that id
/// (it already finished, or it is not one of this backend's cancellable jobs).
#[arbor_rpc::handler]
fn bennu_cancel_job(_state: &BennuState, job_id: String) -> Result<bool, String> {
    // Cloned out of the map before it runs: the action may take its own locks, and must not do so under this one.
    let action = actions().lock().map_err(|e| e.to_string())?.get(&job_id).cloned();
    match action {
        Some(action) => {
            action();
            Ok(true)
        }
        None => Ok(false),
    }
}
