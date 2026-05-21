//! In-memory ring buffer for `arbor.log.*` calls.
//!
//! `api.rs` already routes plugin log calls through `tracing::*` for the dev
//! console.  This module adds a parallel sink that the frontend can subscribe
//! to: every call appends an entry to a bounded buffer and emits a Tauri
//! event so the Plugin Logs panel can stream new lines without polling.
//!
//! The buffer is intentionally simple — global cap, oldest-first eviction.
//! Plugin authors who need persistent / per-plugin retention should write to
//! their own files.

use std::collections::VecDeque;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// Hard cap on the number of entries kept in memory.
const MAX_ENTRIES: usize = 5000;

/// One log line emitted by a plugin via `arbor.log.<level>(msg)`.
#[derive(Debug, Clone, Serialize)]
pub struct PluginLogEntry {
    /// Monotonic sequence number — frontend uses it as a stable list key
    /// and to dedupe across reconnects.
    pub seq:     u64,
    /// Wall-clock unix-ms timestamp.
    pub ts_ms:   u64,
    /// "debug" | "info" | "warn" | "error".
    pub level:   String,
    pub plugin:  String,
    pub message: String,
    /// Pipeline display name when the entry was produced by mirroring a
    /// pipeline step's captured output. `None` for plain `arbor.log.*`
    /// calls. Drives the "filter by pipeline" / "clear pipeline logs"
    /// affordances in the Plugin Logs panel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    /// Pipeline run id when applicable (same gating as `pipeline`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id:   Option<String>,
}

#[derive(Default)]
pub struct PluginLogBuffer {
    entries: VecDeque<PluginLogEntry>,
    counter: u64,
}

impl PluginLogBuffer {
    pub fn push(
        &mut self,
        level:    &str,
        plugin:   &str,
        message:  String,
        pipeline: Option<String>,
        run_id:   Option<String>,
    ) -> PluginLogEntry {
        self.counter += 1;
        let entry = PluginLogEntry {
            seq:     self.counter,
            ts_ms:   std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            level:   level.to_string(),
            plugin:  plugin.to_string(),
            message,
            pipeline,
            run_id,
        };
        self.entries.push_back(entry.clone());
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
        entry
    }

    pub fn snapshot(&self) -> Vec<PluginLogEntry> {
        self.entries.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop every entry whose `pipeline` field equals `name`. Used by the
    /// "Clear pipeline logs" button so the user can wipe a noisy run
    /// without nuking the whole buffer (which still owns plain
    /// `arbor.log.*` entries from the rest of the session).
    pub fn clear_by_pipeline(&mut self, name: &str) {
        self.entries.retain(|e| e.pipeline.as_deref() != Some(name));
    }
}

/// Append a log entry from a non-pipeline source (`arbor.log.*` direct
/// call). Pipeline mirroring goes through `record_with_pipeline` so each
/// entry is tagged for filtering.
pub fn record(app: &AppHandle, level: &str, plugin: &str, message: String) {
    record_inner(app, level, plugin, message, None, None);
}

/// Same as `record` but tags the entry with the pipeline name and run id
/// it originated from. Drives per-pipeline filter / clear in the panel.
pub fn record_with_pipeline(
    app:      &AppHandle,
    level:    &str,
    plugin:   &str,
    message:  String,
    pipeline: &str,
    run_id:   &str,
) {
    record_inner(app, level, plugin, message, Some(pipeline.to_string()), Some(run_id.to_string()));
}

fn record_inner(
    app:      &AppHandle,
    level:    &str,
    plugin:   &str,
    message:  String,
    pipeline: Option<String>,
    run_id:   Option<String>,
) {
    let state = app.state::<AppState>();
    let entry = match state.plugin_logs.lock() {
        Ok(mut b) => b.push(level, plugin, message, pipeline, run_id),
        Err(e) => {
            tracing::error!("plugin_logs mutex poisoned: {e}");
            return;
        }
    };
    let _ = app.emit("arbor://plugin-log", &entry);
}
