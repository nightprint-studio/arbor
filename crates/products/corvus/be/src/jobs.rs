//! `JobHandle` — drives a job in the shell's `JobRegistry` over the reverse
//! channel (ADR-3). corvus-be keeps **no** local registry: the shell's registry
//! is the single source of job state + cancellation. A handler registers a job
//! (the shell mints the id), then reports progress/terminal state through these
//! methods. The user-facing `arbor://job-*` events are emitted by the calling
//! handler via [`CorvusState`](corvus_core::prelude::CorvusState)'s event sink
//! (re-emitted by the shell) — byte-identical to the in-process copy. This type
//! only carries the *registry* state across the boundary, never the events.

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
    /// Register `spec` in the shell's registry and return a handle. The shell
    /// assigns the id and stamps the start time, and the job enters `Running`.
    pub fn register(host: Arc<dyn HostCaller>, spec: JobSpec) -> Result<JobHandle, String> {
        let value = serde_json::to_value(spec).map_err(|e| e.to_string())?;
        let id = host.call("__job_register", value)?;
        let id: String = serde_json::from_value(id).map_err(|e| e.to_string())?;
        Ok(JobHandle { host, id })
    }

    /// A second handle to the same already-registered job (shares the host
    /// channel + shell id), so a spawned worker and its spawner can both drive
    /// the job's terminal status — e.g. the worker reports done/failed, while the
    /// spawner still owns a handle to fail the job if the thread spawn itself
    /// errors. Cheap: clones an `Arc` + a `String`.
    pub fn clone_handle(&self) -> JobHandle {
        JobHandle { host: Arc::clone(&self.host), id: self.id.clone() }
    }

    /// Append an output line to the job's buffer. Registry-only — the caller is
    /// responsible for emitting `arbor://job-output` for the live stream.
    pub fn append(&self, line: &str) {
        let _ = self
            .host
            .call("__job_append", json!({ "job_id": self.id, "line": line }));
    }

    /// Set the job's status (terminal or running). Registry-only — the caller is
    /// responsible for emitting `arbor://job-finished` on a terminal status.
    pub fn set_status(&self, status: JobStatus) {
        let _ = self
            .host
            .call("__job_set_status", json!({ "job_id": self.id, "status": status }));
    }

    /// Whether the user has requested cancellation of this job. The cancel half
    /// of the ADR-3 contract — unused until a *cancellable* OOP job exists (the
    /// two current consumers are `non_cancellable`), kept so that handler needs
    /// no foundation change.
    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        self.host
            .call("__job_is_cancelled", json!(self.id))
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or(false)
    }
}
