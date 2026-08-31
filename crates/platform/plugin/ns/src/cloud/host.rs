//! [`CloudHostOps`] — the twenty `__cloud_*` forwards, in one place.
//!
//! Each method names a shell handler, hands it the JSON the Lua closure decoded, and shapes
//! the reply. They lived as an `NsHost` impl inside `corvus-be`, which meant a second product
//! could only have the cloud by writing them again — twenty chances for a method name to
//! drift, on a seam where a typo surfaces to the user as a Lua `nil` from an op that used to
//! work. The round-trip itself is [`HostProxy`]; what is here is only the vocabulary.
//!
//! ⚠️ **Staging post, not a home.** The cloud is being moved out of Arbor and into the
//! `cloud-storage` plugin and its WASI providers; when that lands, this module leaves with
//! it. Do not grow it — a new cloud capability belongs in the plugin.

use serde_json::{json, Value};

use crate::proxy::HostProxy;

/// Reverse-channel proxy for the shell's cloud stack.
#[derive(Clone)]
pub struct CloudHostOps {
    host: HostProxy,
}

impl CloudHostOps {
    /// Wrap a backend's reverse channel (`App::host_caller()`).
    pub fn new(host: std::sync::Arc<dyn arbor_ipc::prelude::HostCaller>) -> Self {
        Self { host: HostProxy::new(host) }
    }

    // ── secrets ────────────────────────────────────────────────────────────

    pub fn secret_set(&self, secret_ref: &str, value: &str) -> Result<(), String> {
        self.host.unit("__cloud_secret_set", json!({ "secret_ref": secret_ref, "value": value }))
    }

    pub fn secret_exists(&self, secret_ref: &str) -> Result<bool, String> {
        self.host.flag("__cloud_secret_exists", json!({ "secret_ref": secret_ref }))
    }

    pub fn secret_delete(&self, secret_ref: &str) -> Result<(), String> {
        self.host.unit("__cloud_secret_delete", json!({ "secret_ref": secret_ref }))
    }

    // ── connections ────────────────────────────────────────────────────────

    pub fn test_connection(&self, opts: Value) -> Result<Value, String> {
        self.host.call("__cloud_test_connection", opts)
    }

    pub fn test_connection_async(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_test_connection_async", opts)
    }

    // ── listings ───────────────────────────────────────────────────────────

    pub fn list(&self, opts: Value) -> Result<Value, String> {
        self.host.call("__cloud_list", opts)
    }

    pub fn list_stream(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_list_stream", opts)
    }

    pub fn search_stream(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_search_stream", opts)
    }

    pub fn cancel(&self, stream_id: &str) -> Result<(), String> {
        self.host.unit("__cloud_cancel", json!({ "stream_id": stream_id }))
    }

    pub fn is_cancelled(&self, stream_id: &str) -> Result<bool, String> {
        self.host.flag("__cloud_is_cancelled", json!({ "stream_id": stream_id }))
    }

    // ── objects ────────────────────────────────────────────────────────────

    pub fn stat(&self, opts: Value) -> Result<Value, String> {
        self.host.call("__cloud_stat", opts)
    }

    pub fn delete(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_delete", opts)
    }

    pub fn copy(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_copy", opts)
    }

    // ── transfers ──────────────────────────────────────────────────────────

    pub fn download(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_download", opts)
    }

    pub fn upload(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_upload", opts)
    }

    pub fn sync(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_sync", opts)
    }

    pub fn download_many(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_download_many", opts)
    }

    pub fn concat_files(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_concat_files", opts)
    }

    // ── chunk handlers driving the shell's operations card ─────────────────

    pub fn report_progress(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_report_progress", opts)
    }

    pub fn report_done(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_report_done", opts)
    }

    pub fn pick_chunk_order(&self, opts: Value) -> Result<(), String> {
        self.host.unit("__cloud_pick_chunk_order", opts)
    }

    // ── oauth ──────────────────────────────────────────────────────────────

    pub fn oauth_start(&self, opts: Value) -> Result<String, String> {
        self.host.text("__cloud_oauth_start", opts)
    }
}
