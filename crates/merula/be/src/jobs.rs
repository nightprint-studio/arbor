//! `JobHandle` — drives a background job in the shell's `JobRegistry` over the
//! reverse channel (ADR-3), the merula twin of `corvus-be`'s `be/src/jobs.rs`.
//!
//! merula-be keeps **no** local registry: the shell's registry is the single
//! source of job state + cancellation. A handler registers a job (the shell mints
//! the id), reports progress/output, and stamps the terminal status through these
//! methods. Unlike corvus-be, every merula job is **hidden** and **routed to the
//! merula window** (`target = "merula"`) — the user-facing surface is the merula
//! *Downloads & Exports* overlay (live %, phase, cancel, reveal), so a visible
//! Jobs entry would duplicate it. The job is still registered so cancel + the
//! terminal event have a registry entry (revealable via "Show hidden").
//!
//! The user-facing `arbor://job-*` events are emitted by this handle through the
//! state's event sink (re-emitted by the shell) — byte-identical to the in-process
//! `render.rs` / `packs/download.rs` copies. This type carries both the *registry*
//! state across the boundary (via [`HostCaller`]) and the *event* egress (via
//! [`EventSink`]), so a single handle is all a job worker needs.

use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::json;

/// The standard merula job category for a kind of background work, used as both the
/// registry `category` and the `arbor://job-started` payload field.
pub mod category {
    pub const RENDERS: &str = "Renders";
    pub const DOWNLOADS: &str = "Downloads";
    pub const IMPORTS: &str = "Imports";
}

/// A live job in the shell registry, addressed by the id the shell assigned, with
/// the event egress wired in so the worker can emit the `arbor://job-*` lifecycle
/// events itself.
pub struct JobHandle {
    host: Arc<dyn HostCaller>,
    sink: Arc<dyn EventSink>,
    pub id: String,
}

impl JobHandle {
    /// Register a **hidden, merula-routed** job in the shell's registry, emit
    /// `arbor://job-started`, and return a handle. The shell assigns the id and
    /// stamps the start time, and the job enters `Running`.
    ///
    /// `name` is the human label, `command` the one-line description, and
    /// `category` one of [`category`]. Errors only when the reverse channel reply
    /// fails (the caller bails); the visible UI surface is the *Downloads & Exports*
    /// overlay, not the Jobs panel.
    pub fn register(
        host: Arc<dyn HostCaller>,
        sink: Arc<dyn EventSink>,
        name: &str,
        command: &str,
        category: &str,
    ) -> Result<JobHandle, String> {
        let spec = JobSpec {
            name: name.to_string(),
            plugin_name: "merula".to_string(),
            command: command.to_string(),
            category: Some(category.to_string()),
            non_cancellable: false,
            // Hidden + routed to the merula window: the user-facing surface is the
            // merula Downloads & Exports overlay (live %, phase, cancel, reveal), so
            // a Jobs entry would duplicate it. Still registered (tracked, revealable
            // via "Show hidden") to keep the job-event invariants intact.
            hidden: true,
            is_system: false,
            target: Some("merula".to_string()),
        };
        let value = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
        let id = host.call("__job_register", value)?;
        let id: String = serde_json::from_value(id).map_err(|e| e.to_string())?;
        sink.emit(
            "arbor://job-started",
            json!({
                "job_id":      &id,
                "name":        name,
                "plugin_name": "merula",
                "command":     command,
                "category":    category,
                "hidden":      true,
                "target":      "merula",
            }),
        );
        Ok(JobHandle { host, sink, id })
    }

    /// A second handle to the same already-registered job (shares the host channel
    /// + event sink + shell id), so a spawned worker and its spawner can both drive
    /// the job. Cheap: clones two `Arc`s + a `String`.
    pub fn clone_handle(&self) -> JobHandle {
        JobHandle {
            host: Arc::clone(&self.host),
            sink: Arc::clone(&self.sink),
            id: self.id.clone(),
        }
    }

    /// Append an output line to the job's registry buffer (no event — the live
    /// stream rides on the dedicated progress events below).
    pub fn append(&self, line: &str) {
        let _ = self
            .host
            .call("__job_append", json!({ "job_id": self.id, "line": line }));
    }

    /// Whether the user has requested cancellation of this job. Polled by the
    /// render / import / download loops between blocks so `cancel_job` (the overlay
    /// Stop button) stops the work instead of running to completion.
    pub fn is_cancelled(&self) -> bool {
        self.host
            .call("__job_is_cancelled", json!(self.id))
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or(false)
    }

    /// Emit `arbor://job-progress` with a whole-percent value (the throttling — only
    /// emitting on a percentage change — is the caller's concern, mirroring the
    /// in-process render/import loops).
    pub fn emit_progress(&self, pct: i32) {
        self.sink
            .emit("arbor://job-progress", json!({ "job_id": self.id, "pct": pct }));
    }

    /// Mark the job `Completed { exit_code: 0 }` in the registry and emit
    /// `arbor://job-done` with `success = true`.
    pub fn finish_ok(&self) {
        self.set_status(JobStatus::Completed { exit_code: 0 });
        self.emit_done(true, None);
    }

    /// Mark the job `Cancelled` in the registry and emit `arbor://job-done` with
    /// `success = false` (no error — a user cancel is not a failure).
    pub fn finish_cancelled(&self) {
        self.set_status(JobStatus::Cancelled);
        self.emit_done(false, None);
    }

    /// Mark the job `Failed { error }` in the registry and emit `arbor://job-done`
    /// with `success = false` + the surfaced error message.
    pub fn finish_failed(&self, error: String) {
        self.set_status(JobStatus::Failed { error: error.clone() });
        self.emit_done(false, Some(error));
    }

    /// Set the job's registry status (no event — pair with [`emit_done`](Self::emit_done)
    /// on a terminal status, or use the `finish_*` helpers).
    fn set_status(&self, status: JobStatus) {
        let _ = self
            .host
            .call("__job_set_status", json!({ "job_id": self.id, "status": status }));
    }

    /// Emit the terminal `arbor://job-done` event (the `error` key matches the
    /// in-process payload: present only on a failure).
    fn emit_done(&self, success: bool, error: Option<String>) {
        self.sink.emit(
            "arbor://job-done",
            json!({ "job_id": self.id, "success": success, "error": error }),
        );
    }
}
