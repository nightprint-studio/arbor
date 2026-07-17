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

/// Emit granularity when a transfer's response carries no `Content-Length`: a
/// percentage is unknowable, so the loop throttles on bytes received instead of on
/// whole percents.
pub const PROGRESS_BYTE_STEP: u64 = 4 * 1024 * 1024;

/// Decides when a per-chunk transfer loop may emit, so a multi-GB download doesn't
/// emit once per chunk: on each new whole percent when `total` is known, every
/// [`PROGRESS_BYTE_STEP`] bytes when it isn't.
///
/// The unknown-total case is load-bearing, not theoretical: GitHub generates the
/// `archive/refs/heads/*.zip` sample packs on the fly and streams them **chunked with
/// no `Content-Length`**, so `total` is 0 for most of the pack table. Tracking the
/// last percent as a bare `-1`-initialised int silently broke exactly that case —
/// `-1` is also the "unknown percent" value, so the first comparison always matched
/// and *every* emit was suppressed, leaving the transfer stuck on "Starting…" for the
/// whole download. Hence `Option`: "nothing emitted yet" must not be spellable as a
/// real percentage.
#[derive(Default)]
pub struct ProgressThrottle {
    last_pct: Option<i64>,
    last_emit_bytes: Option<u64>,
}

impl ProgressThrottle {
    /// Whether to emit for `done`/`total` now, recording the decision.
    pub fn should_emit(&mut self, done: u64, total: u64) -> bool {
        if total > 0 {
            let pct = ((done as f64 / total as f64) * 100.0) as i64;
            if self.last_pct == Some(pct) {
                return false;
            }
            self.last_pct = Some(pct);
            return true;
        }
        match self.last_emit_bytes {
            Some(prev) if done.saturating_sub(prev) < PROGRESS_BYTE_STEP => false,
            _ => {
                self.last_emit_bytes = Some(done);
                true
            }
        }
    }
}

/// The whole-percent value for `done`/`total`, or `-1` when `total` is unknown (the
/// wire convention every merula progress payload uses for "indeterminate").
pub fn percent_of(done: u64, total: u64) -> i64 {
    if total > 0 {
        ((done as f64 / total as f64) * 100.0) as i64
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression this type exists for: with no `Content-Length` (`total == 0`)
    /// the old `-1`-sentinel throttle suppressed every emit.
    #[test]
    fn unknown_total_emits_on_byte_steps() {
        let mut t = ProgressThrottle::default();
        assert!(t.should_emit(1, 0), "first chunk must emit");
        assert!(!t.should_emit(2, 0), "a byte later is below the step");
        assert!(t.should_emit(1 + PROGRESS_BYTE_STEP, 0), "a full step later emits");
    }

    #[test]
    fn known_total_emits_once_per_whole_percent() {
        let mut t = ProgressThrottle::default();
        assert!(t.should_emit(0, 1000), "0% is a new percent");
        assert!(!t.should_emit(9, 1000), "still 0%");
        assert!(t.should_emit(10, 1000), "1% is a new percent");
        assert!(!t.should_emit(10, 1000), "same percent again");
    }

    #[test]
    fn percent_is_indeterminate_without_a_total() {
        assert_eq!(percent_of(500, 1000), 50);
        assert_eq!(percent_of(500, 0), -1);
    }
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

    /// A second handle to the same already-registered job (shares the host
    /// channel, event sink, and shell id), so a spawned worker and its spawner can
    /// both drive the job. Cheap: clones two `Arc`s + a `String`.
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
