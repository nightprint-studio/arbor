//! [`BackendAppCtx`] — the [`AppCtx`] every headless Model-D backend uses.
//!
//! Event egress goes through the process's [`EventSink`] (a frame the shell
//! re-emits to the FE) instead of a `tauri::AppHandle`, and background futures
//! spawn on the backend's Tokio runtime. The UI/global capabilities a headless
//! backend can't satisfy — `is_focused`, the plugin-log buffer, OS `open_path`,
//! host built-in commands — keep the trait's safe defaults until a launcher
//! round-trip channel lands. Product-agnostic: any `*-be` builds one the same way.

use std::any::Any;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use arbor_core::prelude::AppCtx;
use arbor_ipc::prelude::{EventSink, HostCaller};

/// The plugin host's view of the process, for a headless backend.
pub struct BackendAppCtx {
    sink: Arc<dyn EventSink>,
    runtime: tokio::runtime::Handle,
    dir: PathBuf,
    /// Reverse channel to the shell. Backs the UI capabilities a headless
    /// backend can't satisfy itself — today `open_path`, which forwards to the
    /// shell's `__open_path` host handler (it owns the window + OS opener + the
    /// user's OS-vs-built-in explorer preference). `None` leaves those methods
    /// on the trait's safe defaults (headless / test hosts with no channel).
    host: Option<Arc<dyn HostCaller>>,
}

impl BackendAppCtx {
    /// Build the context from the backend's event sink and runtime handle. The
    /// Arbor data root is resolved once here (same value the shell's `AppCtx`
    /// reports), so `arbor_dir` can hand out a borrow. No reverse channel — use
    /// [`with_host_caller`](Self::with_host_caller) to enable shell round-trips.
    pub fn new(sink: Arc<dyn EventSink>, runtime: tokio::runtime::Handle) -> Self {
        Self { sink, runtime, dir: arbor_core::prelude::arbor_config_dir(), host: None }
    }

    /// Attach the reverse channel so UI capabilities the backend can't satisfy
    /// locally (e.g. `open_path`) forward to the shell.
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }
}

impl AppCtx for BackendAppCtx {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.sink.emit(event, payload);
    }

    fn spawn(&self, fut: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        // A captured runtime `Handle` spawns from any thread (the plugin-boot
        // thread has no ambient reactor), mirroring the shell's
        // `tauri::async_runtime::spawn`.
        self.runtime.spawn(fut);
    }

    fn arbor_dir(&self) -> &Path {
        &self.dir
    }

    fn is_focused(&self) -> bool {
        // No window in a headless backend; focus-gated loops back off safely on
        // `false` (a launcher will broadcast real focus later).
        false
    }

    /// Announce a plugin's log line so the shell can file it.
    ///
    /// The trait's default is a no-op, and inheriting it is what made the Plugin Logs panel
    /// permanently empty: since the plugin hosts moved into the product backends, every
    /// `arbor.log.*` call in the app was made through THIS context — and went nowhere, with
    /// the panel showing an empty list rather than an error, so nothing said the channel was
    /// gone. See [`PLUGIN_LOG_TOPIC`] for why the entry is finished shell-side.
    fn record_plugin_log(&self, level: &str, plugin: &str, message: &str) {
        self.sink.emit(
            arbor_ipc::prelude::PLUGIN_LOG_TOPIC,
            serde_json::json!({ "level": level, "plugin": plugin, "message": message }),
        );
    }

    // Extensions run in the shell, which owns the wasm engine — a second engine per backend
    // would compile every module again and give a plugin a different instance depending on
    // which process it happened to load in. Blocks on the reply, like `open_path`.
    fn ext_surface(&self, _plugin: &str) -> Result<String, String> {
        match &self.host {
            Some(h) => h
                .call("__ext_surface", serde_json::json!({}))
                .map(|v| v.to_string()),
            None => Err("arbor.ext: no reverse channel to the shell".to_string()),
        }
    }

    fn ext_call(&self, plugin: &str, spec_json: &str) -> Result<String, String> {
        let spec: serde_json::Value =
            serde_json::from_str(spec_json).map_err(|e| format!("arbor.ext.call: {e}"))?;
        match &self.host {
            Some(h) => h
                .call("__ext_call", serde_json::json!({ "plugin": plugin, "spec": spec }))
                .map(|v| v.to_string()),
            None => Err("arbor.ext: no reverse channel to the shell".to_string()),
        }
    }

    fn open_path(&self, path: &str) -> Result<(), String> {
        // A headless backend has no window / OS opener, so forward to the
        // shell's `__open_path` handler, which applies the user's OS-vs-built-in
        // explorer preference and reveals the path. Blocks on the reply.
        match &self.host {
            Some(h) => h
                .call("__open_path", serde_json::json!({ "path": path }))
                .map(|_| ()),
            None => Err("open_path: no reverse channel to the shell".to_string()),
        }
    }
}
