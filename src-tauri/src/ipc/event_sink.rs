//! In-process backing for the Model-D event egress.
//!
//! [`TauriEventSink`] is the shell-side implementation of
//! [`arbor_ipc::prelude::EventSink`]: it forwards `emit` straight to
//! `AppHandle::emit`. It's what `CorvusState` holds while Corvus runs in-process.
//! When `corvus-be` splits into its own process, the backend instead holds a
//! sink that wraps the `arbor-ipc` event channel (each `emit` → an
//! `Event::Notify` the shell re-emits) — the handler call sites
//! (`state.emit(...)`) don't change.

use arbor_ipc::prelude::EventSink;
use tauri::{AppHandle, Emitter};

/// Forwards backend events to the frontend via `AppHandle::emit`.
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, topic: &str, payload: serde_json::Value) {
        if let Err(e) = self.app.emit(topic, payload) {
            tracing::warn!("TauriEventSink::emit('{topic}') failed: {e}");
        }
    }
}
