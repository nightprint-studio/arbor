//! `JobHandle` — drives a job in the shell's `JobRegistry` over the reverse channel
//! (ADR-3). bennu-be keeps **no** local registry: the shell's registry is the single
//! source of job state. A handler (here: the whole-project analysis warm-up) registers
//! a job — the shell mints the id — then reports terminal state through these methods.
//! The user-facing `arbor://job-*` events are emitted separately by the caller via the
//! [`BennuState`](bennu_core::prelude::BennuState) event sink (re-emitted by the shell);
//! this type carries only the *registry* state across the boundary, never the events.
//!
//! A near-verbatim twin of `corvus-be`'s `JobHandle` (same reverse-channel contract).

use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::HostCaller;
use serde_json::json;

/// A live job in the shell registry, addressed by the id the shell assigned.
pub struct JobHandle {
    host: Arc<dyn HostCaller>,
    pub id: String,
}

impl JobHandle {
    /// Register `spec` in the shell's registry and return a handle. The shell assigns the
    /// id and stamps the start time, and the job enters `Running`.
    pub fn register(host: Arc<dyn HostCaller>, spec: JobSpec) -> Result<JobHandle, String> {
        let value = serde_json::to_value(spec).map_err(|e| e.to_string())?;
        let id = host.call("__job_register", value)?;
        let id: String = serde_json::from_value(id).map_err(|e| e.to_string())?;
        Ok(JobHandle { host, id })
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
