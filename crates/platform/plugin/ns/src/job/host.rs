//! [`JobHostOps`] — the `__job_*` forwards behind `arbor.job`.
//!
//! The shell owns the one `JobRegistry` and the processes it tracks; a backend only asks.
//! Five ops plus the id reservation, and the reservation is first because the job id has to
//! exist before the spawn: the synthetic `on_done` hook name and the `arbor://job-started`
//! payload both carry it.

use serde_json::{json, Value};

use crate::proxy::HostProxy;

/// Reverse-channel proxy for the shell's job registry.
#[derive(Clone)]
pub struct JobHostOps {
    host: HostProxy,
}

impl JobHostOps {
    /// Wrap a backend's reverse channel (`App::host_caller()`).
    pub fn new(host: std::sync::Arc<dyn arbor_ipc::prelude::HostCaller>) -> Self {
        Self { host: HostProxy::new(host) }
    }

    /// Reserve a job id, registering a Running `JobInfo` with it, so the caller can name the
    /// job before it starts.
    pub fn new_id(
        &self,
        name: &str,
        plugin_name: &str,
        command: &str,
        category: Option<&str>,
        hidden: bool,
        target: Option<&str>,
    ) -> Result<String, String> {
        let spec = json!({
            "name": name,
            "plugin_name": plugin_name,
            "command": command,
            "category": category,
            "non_cancellable": false,
            "hidden": hidden,
            "is_system": false,
            "target": target,
        });
        let v = self.host.call("__job_register", spec)?;
        v.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "job.spawn: __job_register returned non-string id".to_string())
    }

    /// Drive the real process spawn for an already-reserved job.
    pub fn spawn(&self, spec: Value) -> Result<(), String> {
        self.host.unit("__job_spawn", spec)
    }

    /// The job list, serde-serialized as a JSON array.
    pub fn list(&self) -> Result<Value, String> {
        self.host.call("__job_list", json!({}))
    }

    /// Best-effort cancel — the Lua surface never fails on this.
    pub fn cancel(&self, job_id: &str) -> Result<(), String> {
        self.host.unit("__job_cancel", json!({ "job_id": job_id }))
    }

    /// Drop a terminal-state job; `true` when removed, `false` for running / unknown.
    pub fn dismiss(&self, job_id: &str) -> Result<bool, String> {
        self.host.flag("__job_dismiss", json!({ "job_id": job_id }))
    }

    /// Drop every terminal-state job; returns the ids dismissed.
    pub fn clear_finished(&self) -> Result<Vec<String>, String> {
        let v = self.host.call("__job_clear_finished", json!({}))?;
        serde_json::from_value(v).map_err(|e| format!("job.clear_finished decode: {e}"))
    }
}
