//! Bennu's adapter onto the generic `arbor-plugin-rpc` surface.
//!
//! The Plugin Manager talks to whichever backend hosts the plugins — `list_plugin_info`,
//! `enable_plugin`, `reload_plugins`, the dependency graph, the per-plugin settings. That
//! logic lives once in `arbor-plugin-rpc`, generic over a [`PluginRpcContext`]; the orphan
//! rule forbids implementing a foreign trait for the foreign `BennuState`, so this is a
//! **local** newtype that carries the impl.
//!
//! Without it bennu-be serves none of those methods, and the Plugin Manager opened from Bennu
//! answers `unknown command: list_plugin_info` and renders "No plugins found" — a window that
//! runs plugins but cannot say which.
//!
//! ## Where bennu differs from corvus
//!
//! Two of the trait's methods are about **open repositories**, which is a git idea. Bennu has
//! projects instead, and the mapping is honest rather than clever: a plugin asking "what is
//! open?" gets bennu's open projects, because that is the same question in bennu's terms. The
//! shape a plugin receives — an id, a path, a name — does not change.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arbor_plugin_core::prelude::PluginHost;
use arbor_plugin_rpc::prelude::{OpenRepo, PluginRpc, PluginRpcContext};
use arbor_rpc::CallFn;
use bennu_core::state::BennuState;
use serde_json::Value;

/// Local adapter holding the shared state. Built per plugin-method dispatch — an `Arc` clone.
pub struct BennuRpcCtx {
    state: Arc<BennuState>,
}

impl BennuRpcCtx {
    pub fn new(state: Arc<BennuState>) -> Self {
        Self { state }
    }
}

impl PluginRpcContext for BennuRpcCtx {
    fn plugin_host(&self) -> Arc<Mutex<PluginHost>> {
        crate::host_handle::host()
    }

    fn emit(&self, event: &str, payload: Value) {
        self.state.emit(event, payload);
    }

    fn fire_hook(&self, hook: &str, payload: Value) {
        // Bennu has no hook catalog of its own yet, so the only hooks that reach a plugin here
        // are the `arbor:` lifecycle ones the runtime fires itself. Routed through the host
        // rather than dropped: the day bennu declares its own events, this line is already the
        // right one and the catalog is the only thing that changes.
        match self.plugin_host().lock() {
            Ok(host) => arbor_plugin_core::prelude::fire_broadcast(&host, hook, &payload.to_string()),
            Err(e) => eprintln!("bennu-be: plugin host poisoned, dropping hook {hook}: {e}"),
        }
    }

    fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.state.host_call(method, params)
    }

    fn repo_path(&self, _tab_id: &str) -> Option<String> {
        // Bennu has one workspace at a time rather than a tab per project, so a tab id does
        // not select anything: the answer is the active project either way. `None` when
        // nothing is open, which is what a plugin has to handle regardless.
        crate::project::active_root()
    }

    fn open_repos(&self) -> Vec<OpenRepo> {
        crate::project::active_root()
            .map(|path| {
                let name = path
                    .replace('\\', "/")
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(&path)
                    .to_string();
                vec![OpenRepo { tab_id: "bennu".to_string(), path, name }]
            })
            .unwrap_or_default()
    }
}

/// The plugin RPC sync method map, monomorphised for [`BennuRpcCtx`].
pub fn methods() -> HashMap<&'static str, CallFn> {
    let (sync, _async) = arbor_rpc::Builder::<BennuRpcCtx>::new()
        .add(PluginRpc)
        .into_maps();
    sync
}
