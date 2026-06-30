//! [`SittaState`] — the headless sitta backend's owned state.
//!
//! Mirrors `merula-core`'s `MerulaState` in spirit but minimal: a file explorer
//! keeps no long-lived domain state of its own (FS lives in `arbor-fs`, git in
//! `corvus-git`), so this is just the BE→FE event egress plus the reverse channel
//! back to the shell. New pieces gain a `with_*` builder rather than a new
//! constructor, so a later wave never has to re-edit this file.

use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::Value;

/// The state every sitta-be handler gets, `Arc`-shared across the dispatcher and
/// any background workers.
pub struct SittaState {
    /// Backend → frontend event egress. The shell re-emits each topic to the
    /// explorer window(s). Call sites use [`emit`](Self::emit) /
    /// [`event_sink`](Self::event_sink).
    sink: Arc<dyn EventSink>,
    /// Reverse channel back to the shell (`docs/reverse-channel.md`), set from the
    /// `App`'s host caller. Used by handlers that must call into the shell (e.g.
    /// the git-awareness wave routing through `corvus-git`). `None` only in the
    /// (unused) in-process construction path.
    host: Option<Arc<dyn HostCaller>>,
}

impl SittaState {
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

    /// Emit a frontend event. The shell re-emits the topic to the explorer
    /// window(s).
    pub fn emit(&self, topic: &str, payload: Value) {
        self.sink.emit(topic, payload);
    }

    /// A cloneable handle to the event egress, for a background worker that emits
    /// from inside and outlives the borrow of `&self`.
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
