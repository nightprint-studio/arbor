//! [`TytoState`] — the headless tyto backend's owned state.
//!
//! Mirrors `sitta-core`'s `SittaState`: transport-only. A screen recorder's heavy
//! state (capture session, encoder pipeline) lives in the recording engine the
//! domain handlers own; this state carries only the BE→FE event egress and the
//! reverse channel back to the shell. New pieces gain a `with_*` builder rather
//! than a new constructor, so a later wave never has to re-edit this file.
//!
//! NOTE: recorder lifecycle hooks (`on_recording_started`, …) fire through the
//! plugin host's hook dispatcher (bound in `tyto-be/src/plugin.rs`), not through
//! this state — exactly as sitta keeps hooks out of `SittaState`.

use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::Value;

/// The state every tyto-be handler gets, `Arc`-shared across the dispatcher and
/// any background workers (the recording engine thread).
pub struct TytoState {
    /// Backend → frontend event egress. The shell re-emits each topic to the
    /// Tyto window. Call sites use [`emit`](Self::emit) /
    /// [`event_sink`](Self::event_sink).
    sink: Arc<dyn EventSink>,
    /// Reverse channel back to the shell (`docs/reverse-channel.md`), set from the
    /// `App`'s host caller. Used by handlers that must call into the shell (e.g.
    /// reveal-in-explorer / open-path for a saved capture). `None` only in the
    /// (unused) in-process construction path.
    host: Option<Arc<dyn HostCaller>>,
}

impl TytoState {
    /// Build the backend state from its event egress. Wave-friendly: a new piece
    /// gains a `with_*` builder rather than a new constructor.
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self { sink, host: None }
    }

    /// Attach the reverse channel back to the shell (the `App`'s host caller).
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    /// Emit a frontend event. The shell re-emits the topic to the Tyto window.
    pub fn emit(&self, topic: &str, payload: Value) {
        self.sink.emit(topic, payload);
    }

    /// A cloneable handle to the event egress, for a background worker (the
    /// recording engine) that emits from inside and outlives the borrow of `&self`.
    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    /// Call back into the shell, blocking on the reply. Errors with a clear
    /// message when no reverse channel is wired.
    pub fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        match &self.host {
            Some(h) => h.call(method, params),
            None => Err(format!("host_call('{method}'): no reverse channel (in-process)")),
        }
    }

    /// A cloneable handle to the reverse channel, for a background worker.
    pub fn host_caller(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.clone()
    }
}
