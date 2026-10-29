//! Jobs-panel integration. voice-be keeps no registry: the shell's `JobRegistry` is the single
//! source of job state, driven over the reverse channel (the same contract as bennu-be's and
//! corvus-be's `JobHandle`). The user-facing `arbor://job-*` events go out through the sink.

use std::sync::{Arc, OnceLock};

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::json;

static HOST: OnceLock<Arc<dyn HostCaller>> = OnceLock::new();

/// Wire the reverse channel once, at start-up.
pub fn init(host: Arc<dyn HostCaller>) {
    let _ = HOST.set(host);
}

pub struct Job {
    host: Arc<dyn HostCaller>,
    sink: Arc<dyn EventSink>,
    id: String,
}

impl Job {
    /// Register a job; `None` when there is no reverse channel (the work simply goes unlisted).
    pub fn start(sink: &Arc<dyn EventSink>, name: &str, command: &str, category: &str, cancellable: bool) -> Option<Job> {
        let host = Arc::clone(HOST.get()?);
        let spec = JobSpec {
            name: name.to_string(),
            plugin_name: "voice".into(),
            command: command.to_string(),
            category: Some(category.to_string()),
            non_cancellable: !cancellable,
            hidden: false,
            is_system: false,
            target: None,
        };
        let id = host.call("__job_register", serde_json::to_value(spec).ok()?).ok()?;
        let id: String = serde_json::from_value(id).ok()?;
        sink.emit(
            "arbor://job-started",
            json!({ "job_id": &id, "name": name, "plugin_name": "voice", "command": command, "category": category }),
        );
        Some(Job { host, sink: Arc::clone(sink), id })
    }

    pub fn finish(self, error: Option<String>) {
        let success = error.is_none();
        let status = match &error {
            None => JobStatus::Completed { exit_code: 0 },
            Some(e) => JobStatus::Failed { error: e.clone() },
        };
        let _ = self.host.call("__job_set_status", json!({ "job_id": self.id, "status": status }));
        self.sink.emit(
            "arbor://job-done",
            json!({
                "job_id": self.id,
                "success": success,
                "exit_code": if success { 0 } else { -1 },
                "cancelled": false,
                "error": error,
            }),
        );
    }
}
