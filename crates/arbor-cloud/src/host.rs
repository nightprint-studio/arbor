//! Boundary trait the host (`src-tauri`) implements so the cloud crate can
//! talk to the JobRegistry, the PluginHost and the Tauri event bus without
//! depending on any of them directly.
//!
//! The host constructs an impl in `setup()` and stores it as
//! `Arc<dyn CloudHost>` in Tauri's managed state; both the Tauri command
//! layer (`commands/cloud_commands.rs`) and the Lua namespace
//! (`plugin/api/ns/cloud.rs`) pull it out by type and pass it into the
//! arbor-cloud functions.
//!
//! Every method is fire-and-forget (no `Result`). Errors that the cloud
//! crate can't act on (poisoned mutex, dropped runtime, missing webview)
//! get logged inside the impl and swallowed at the boundary — there is
//! nothing useful the cloud function calling `host.emit_event(...)` could
//! do with an `Err` that the host hasn't already done.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

/// Per-job cancellation flags. Subprocess-backed jobs are killed by PID;
/// opendal-backed jobs have no PID, so they poll an `AtomicBool` between
/// chunks instead. The map is keyed by `job_id` for transfers and by
/// `stream_id` for streaming list/search operations.
pub type CloudCancellations = Mutex<HashMap<String, Arc<AtomicBool>>>;

/// `stream_id → job_id` map for `download_many` calls that defer their
/// final status report to a follow-up phase (e.g. chunk-merge). The
/// streaming download inserts here when `keep_open = true`; the host's
/// `cloud_report_done` Tauri command reads + removes the entry to finalize
/// the job once the merge phase ends.
pub type CloudPendingOps = Mutex<HashMap<String, String>>;

/// Subset of `crate::jobs::JobInfo` (in `src-tauri`) that the cloud crate
/// needs to populate when it registers a transfer job. The host's
/// `CloudHost` impl converts to the full `JobInfo` shape on the way in.
#[derive(Debug, Clone)]
pub struct CloudJobInfo {
    pub id:              String,
    pub name:            String,
    pub plugin_name:     String,
    pub command:         String,
    pub started_at:      u64,
    pub status:          CloudJobStatus,
    pub category:        Option<String>,
    pub non_cancellable: bool,
    pub hidden:          bool,
    pub is_system:       bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CloudJobStatus {
    Running,
    Completed { exit_code: i32 },
    Failed    { error: String },
    Cancelled,
}

/// Host capabilities the cloud crate consumes. Implemented by `src-tauri`
/// on a struct that closes over the relevant `Arc<Mutex<...>>` registries
/// and a `tauri::AppHandle`.
pub trait CloudHost: Send + Sync + 'static {
    // ── Cancellation registries ─────────────────────────────────────────
    fn cancellations(&self) -> &CloudCancellations;
    fn pending_ops(&self)    -> &CloudPendingOps;

    // ── Plugin hook delivery (Lua subscribers) ──────────────────────────
    //
    // `payload_json` is already serialised — the cloud crate constructs a
    // `serde_json::Value` and `to_string()`s it before calling. This keeps
    // the trait free of serde generics.
    fn fire_plugin_hook(&self, plugin: &str, hook: &str, payload_json: &str);

    // ── Tauri event emission (JS frontend) ──────────────────────────────
    fn emit_event(&self, topic: &str, payload: serde_json::Value);

    // ── JobRegistry forwarders ──────────────────────────────────────────
    //
    // Each call locks the registry briefly inside the impl. The cloud
    // crate never holds a JobRegistry guard across an `await`, so the
    // per-call lock granularity is correct (mirrors the pre-split
    // pattern in `transfer.rs`).
    fn job_new_id(&self) -> String;
    fn job_register(&self, info: CloudJobInfo);
    fn job_append_output(&self, job_id: &str, line: String);
    fn job_set_status(&self, job_id: &str, status: CloudJobStatus);
}
