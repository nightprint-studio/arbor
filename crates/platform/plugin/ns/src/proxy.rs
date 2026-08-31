//! [`HostProxy`] — the shared half of every namespace in this crate.
//!
//! All of these namespaces are the same shape: the state lives in the shell (the job
//! registry, the cloud stack, the OAuth engine + vault), and the plugin host is a separate
//! process, so each Lua call is a `__<domain>_<op>` round-trip on the reverse channel. Only
//! the method names and the reply shapes differ.
//!
//! So the round-trip lives once. What a domain module adds on top is the vocabulary — which
//! method, which arguments, how the reply reads — and that is all it should have to add.

use std::sync::Arc;

use arbor_ipc::prelude::HostCaller;
use serde_json::Value;

/// A backend's reverse channel, wrapped for the namespaces built on it.
///
/// `Clone` because every `install_*` captures its own handle for the closure it registers;
/// the clone is an `Arc` bump.
#[derive(Clone)]
pub struct HostProxy {
    host: Arc<dyn HostCaller>,
}

impl HostProxy {
    /// Wrap a backend's reverse channel (`App::host_caller()`).
    pub fn new(host: Arc<dyn HostCaller>) -> Self {
        Self { host }
    }

    /// Call a shell host handler and hand back its reply verbatim. The error `String` is the
    /// shell's own — it is what the plugin sees, so it must not be reworded here.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.host.call(method, params)
    }

    /// A reply the shell sends as a JSON string, as a Rust `String`. Every id-returning op
    /// (`stream_id`, `job_id`, an OAuth URL) lands here, and an absent or ill-typed reply
    /// becomes `""` rather than an error — the shell handler is the one that reports failure,
    /// through `Err`.
    pub fn text(&self, method: &str, params: Value) -> Result<String, String> {
        Ok(self.call(method, params)?.as_str().unwrap_or_default().to_string())
    }

    /// A reply the shell sends as a JSON bool, defaulting to `false`.
    pub fn flag(&self, method: &str, params: Value) -> Result<bool, String> {
        Ok(self.call(method, params)?.as_bool().unwrap_or(false))
    }

    /// A call whose reply carries nothing worth reading.
    pub fn unit(&self, method: &str, params: Value) -> Result<(), String> {
        self.call(method, params).map(|_| ())
    }
}
