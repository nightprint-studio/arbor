//! The process-wide plugin host handle, published once at boot.
//!
//! `main` builds the `Arc<Mutex<PluginHost>>` and calls [`install`]; the plugin
//! RPC adapter ([`crate::plugin_rpc::BennuRpcCtx`]) reads it via [`host`]. The
//! host is kept **out** of `BennuState` on purpose — it is mlua-coupled and
//! owned by `main`, while `BennuState` stays transport-only, so threading it
//! through the state would drag the whole `plugin-core` dependency into the
//! `bennu-core` crate. Publishing it here keeps the wiring a single line.

use std::sync::{Arc, Mutex, OnceLock};

use arbor_plugin_core::prelude::PluginHost;

static HOST: OnceLock<Arc<Mutex<PluginHost>>> = OnceLock::new();

/// Publish the host. Called once from `main` after construction, before serving.
/// Idempotent — a second call is ignored.
pub fn install(host: Arc<Mutex<PluginHost>>) {
    let _ = HOST.set(host);
}

/// The shared host handle. Panics only if called before [`install`] (a boot
/// ordering bug, surfaced verbatim).
pub fn host() -> Arc<Mutex<PluginHost>> {
    Arc::clone(HOST.get().expect("plugin host not installed in bennu-be"))
}
