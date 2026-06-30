//! Corvus's adapter onto the generic `arbor-plugin-rpc` surface.
//!
//! The whole Plugin-Manager RPC logic (enable/disable cascades, reload, the
//! master toggle, per-plugin schedulers, hook/command dispatch, and the
//! read/dep-graph surface) lives once in `arbor-plugin-rpc`, generic over a
//! [`PluginRpcContext`]. The orphan rule forbids implementing that foreign trait
//! for the foreign `CorvusState` here, so [`CorvusRpcCtx`] is a **local** newtype
//! over the shared state that carries the impl. The generic `PluginRpc` bundle is
//! monomorphised for it in [`methods`]; the serve loop builds a `CorvusRpcCtx`
//! per plugin-method call and dispatches against it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arbor_plugin_core::prelude::PluginHost;
use arbor_plugin_rpc::prelude::{OpenRepo, PluginRpc, PluginRpcContext};
use arbor_rpc::CallFn;
use corvus_core::prelude::CorvusState;
use serde_json::Value;

/// Local adapter holding the shared state, so it can implement the foreign
/// [`PluginRpcContext`]. Built per plugin-method dispatch — cheap, just an `Arc`
/// clone.
pub struct CorvusRpcCtx {
    state: Arc<CorvusState>,
}

impl CorvusRpcCtx {
    pub fn new(state: Arc<CorvusState>) -> Self {
        Self { state }
    }
}

impl PluginRpcContext for CorvusRpcCtx {
    fn plugin_host(&self) -> Arc<Mutex<PluginHost>> {
        crate::host_handle::host()
    }

    fn emit(&self, event: &str, payload: Value) {
        self.state.emit(event, payload);
    }

    fn fire_hook(&self, hook: &str, payload: Value) {
        self.state.fire_hook(hook, payload);
    }

    fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.state.host_call(method, params)
    }

    fn repo_path(&self, tab_id: &str) -> Option<String> {
        self.state.repo_path(tab_id)
    }

    fn open_repos(&self) -> Vec<OpenRepo> {
        let opens = self.state.open_tabs(); // Vec<(tab_id, path)>
        if opens.is_empty() {
            return Vec::new();
        }
        // Resolve a display name from the repo registry (file-backed,
        // reload-on-read), falling back to the path's basename. The guard drops
        // when this scope ends — before any hook fires (the caller re-enters the
        // host).
        let reg = crate::workspace::registry::registry(&self.state);
        opens
            .into_iter()
            .map(|(tab_id, path)| {
                let name = reg
                    .find_by_path(&path)
                    .map(|e| e.display_name.clone())
                    .unwrap_or_else(|| basename(&path));
                OpenRepo { tab_id, path, name }
            })
            .collect()
    }
}

/// Last path segment as a fallback repo name (registry miss).
fn basename(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

/// The plugin RPC sync method map (`enable_plugin`, `reload_plugins`,
/// `list_plugin_info`, …), assembled from the generic `PluginRpc` bundle
/// monomorphised for [`CorvusRpcCtx`]. Every plugin handler is sync, so there is
/// no async map.
pub fn methods() -> HashMap<&'static str, CallFn> {
    let (sync, _async) = arbor_rpc::Builder::<CorvusRpcCtx>::new()
        .add(PluginRpc)
        .into_maps();
    sync
}
