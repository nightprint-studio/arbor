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
use arbor_ipc::prelude::EventSink;

/// The plugin host's view of the process, for a headless backend.
pub struct BackendAppCtx {
    sink: Arc<dyn EventSink>,
    runtime: tokio::runtime::Handle,
    dir: PathBuf,
}

impl BackendAppCtx {
    /// Build the context from the backend's event sink and runtime handle. The
    /// Arbor data root is resolved once here (same value the shell's `AppCtx`
    /// reports), so `arbor_dir` can hand out a borrow.
    pub fn new(sink: Arc<dyn EventSink>, runtime: tokio::runtime::Handle) -> Self {
        Self { sink, runtime, dir: arbor_core::prelude::arbor_config_dir() }
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
}
