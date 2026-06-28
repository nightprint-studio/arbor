//! The host abstraction the generic handlers run against.
//!
//! Every handler in this crate is generic over a [`PluginRpcContext`]: the
//! ambient capabilities a Model-D backend's state already provides (event
//! egress, the plugin hook broker, the reverse channel to the shell, the open
//! repos, tab→path resolution) plus the process-wide plugin host. A product
//! implements this for a **local adapter** over its own state — the orphan rule
//! forbids implementing a foreign trait for a state type owned by another crate,
//! so the adapter (a newtype in the `*-be` binary) is where it lands — then adds
//! [`crate::PluginRpc`] to its [`arbor_rpc::Builder`].

use std::sync::{Arc, Mutex};

use arbor_plugin_core::prelude::PluginHost;
use serde_json::Value;

/// One open repo as the reload path needs it: tab id, repo path, and a display
/// name (resolved by the product — registry label, basename, whatever it has).
pub struct OpenRepo {
    pub tab_id: String,
    pub path: String,
    pub name: String,
}

/// Ambient capabilities the generic Plugin-Manager handlers need from a backend.
///
/// `'static` because the handlers recover the concrete context by
/// `downcast_ref::<C>()` off a type-erased `&dyn Any`, and [`std::any::Any`]
/// requires `'static`.
pub trait PluginRpcContext: 'static {
    /// The process-wide plugin host the backend's `main` built and published.
    fn plugin_host(&self) -> Arc<Mutex<PluginHost>>;

    /// Re-emit a backend→shell event (the shell forwards it to the frontend).
    fn emit(&self, event: &str, payload: Value);

    /// Fire a plugin hook through the host's broker.
    fn fire_hook(&self, hook: &str, payload: Value);

    /// Call back to the shell over the reverse channel (e.g. to persist a flag).
    fn host_call(&self, method: &str, params: Value) -> Result<Value, String>;

    /// The currently-open repos, with display names resolved — drives the reload
    /// path's per-tab `on_repo_open` re-fire.
    fn open_repos(&self) -> Vec<OpenRepo>;

    /// Resolve a tab id to its repo path, for `set_active_tab`.
    fn repo_path(&self, tab_id: &str) -> Option<String>;
}

/// Lock the context's plugin host for a **read**, mapping a poisoned lock onto a
/// stable error string. Shared by every read handler.
pub(crate) fn with_host<C: PluginRpcContext, R>(
    ctx: &C,
    f: impl FnOnce(&PluginHost) -> Result<R, String>,
) -> Result<R, String> {
    let host = ctx.plugin_host();
    let guard = host.lock().map_err(|_| "plugin host mutex poisoned".to_string())?;
    f(&guard)
}

/// Lock the context's plugin host for a **write**. The mutation twin of
/// [`with_host`] — one host, one lock discipline.
pub(crate) fn with_host_mut<C: PluginRpcContext, R>(
    ctx: &C,
    f: impl FnOnce(&mut PluginHost) -> Result<R, String>,
) -> Result<R, String> {
    let host = ctx.plugin_host();
    let mut guard = host.lock().map_err(|_| "plugin host mutex poisoned".to_string())?;
    f(&mut guard)
}
