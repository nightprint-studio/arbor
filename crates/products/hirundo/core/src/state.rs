//! [`HirundoState`] — the headless Hirundo backend's owned state.
//!
//! Transport only, like `TytoState`: the event egress to the window and the reverse channel back to the shell
//! (which is where the keyring is). What Hirundo works on lives on disk in `.hirundo/` ([`crate::store`]);
//! nothing is cached here, so what the window shows is what the files say.

use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::Value;

pub struct HirundoState {
    sink: Arc<dyn EventSink>,
    host: Option<Arc<dyn HostCaller>>,
}

impl HirundoState {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self { sink, host: None }
    }

    /// Attach the reverse channel back to the shell.
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    pub fn emit(&self, topic: &str, payload: Value) {
        self.sink.emit(topic, payload);
    }

    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    /// Call back into the shell, blocking on the reply.
    pub fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        match &self.host {
            Some(h) => h.call(method, params),
            None => Err(format!("host_call('{method}'): no reverse channel (in-process)")),
        }
    }

    pub fn host_caller(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.clone()
    }
}
