//! A running plan as one Job in the shell's Jobs panel: a line per step, a terminal status when
//! the run stops. Every call on the job crosses the reverse channel *synchronously*, so each runs
//! on the blocking pool — a runtime worker parked on it would starve the very reply it waits for.

use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
use serde_json::json;

use crate::jobs::JobHandle;

/// Cheap to clone: the steps of a phase run on their own tasks and each logs to the same job.
#[derive(Clone)]
pub struct PlanJob {
    handle: Option<Arc<JobHandle>>,
    sink: Arc<dyn EventSink>,
}

impl PlanJob {
    /// Register the job. Without a reverse channel (never, out of process) the run simply has no
    /// Jobs entry; the plan itself is unaffected.
    pub async fn start(state: &CorvusState, plan_name: &str) -> PlanJob {
        let sink = state.event_sink();
        let Some(host) = state.host_caller() else { return PlanJob { handle: None, sink } };
        let name = format!("Plan: {plan_name}");
        let spec = JobSpec {
            name: name.clone(),
            plugin_name: "arbor".into(),
            command: "run plan".into(),
            category: Some("Plan".into()),
            non_cancellable: true,
            hidden: false,
            is_system: false,
            target: None,
        };
        let handle = tokio::task::spawn_blocking(move || JobHandle::register(host, spec)).await.ok().and_then(Result::ok);
        if let Some(h) = &handle {
            sink.emit("arbor://job-started", json!({
                "job_id": &h.id, "name": name, "plugin_name": "arbor", "command": "run plan", "category": "Plan",
            }));
        }
        PlanJob { handle: handle.map(Arc::new), sink }
    }

    /// One output line, in the Jobs panel's buffer and on the live stream.
    pub fn line(&self, text: impl Into<String>) {
        let Some(h) = &self.handle else { return };
        let text = text.into();
        self.sink.emit("arbor://job-output", json!({ "job_id": &h.id, "text": &text }));
        let h = Arc::clone(h);
        tokio::task::spawn_blocking(move || h.append(&text));
    }

    /// Close the job: `Ok` completes it, `Err(reason)` fails it (a paused plan ends its run failed;
    /// resuming opens a new job).
    pub async fn finish(&self, outcome: Result<(), String>) {
        let Some(h) = self.handle.clone() else { return };
        let (status, success) = match &outcome {
            Ok(()) => (JobStatus::Completed { exit_code: 0 }, true),
            Err(e) => (JobStatus::Failed { error: e.clone() }, false),
        };
        let id = h.id.clone();
        let _ = tokio::task::spawn_blocking(move || h.set_status(status)).await;
        self.sink.emit("arbor://job-done", json!({ "job_id": id, "success": success, "exit_code": if success { 0 } else { -1 } }));
    }
}
