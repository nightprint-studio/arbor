//! Build the runtime hook dispatcher for an Arbor plugin host.

use std::sync::{Arc, Mutex};

use arbor_plugin_api::prelude::{HookDef, HookDispatcher, HookKind};

use crate::hook_router::LuaHookListener;
use crate::runtime::PluginHost;

/// Build the [`HookDispatcher`] for a plugin host: register every hook in the
/// static [`HOOK_CATALOG`](arbor_plugin_types::prelude::HOOK_CATALOG) (marking
/// `on_pre_commit` vetoable, the rest fire-and-forget) and bind a single
/// [`LuaHookListener`] to `plugin_host`.
///
/// Product-agnostic — every host builds its dispatcher through this one function
/// (the launcher's in-process host, the headless `corvus-be` / `sitta-be` hosts),
/// so a fire from any side fans out identically: the listener walks the same
/// `PluginHost` shape everywhere. (Lives here rather than in a product crate
/// because it depends only on the plugin foundations — the catalog, the hook
/// types, and the listener.)
pub fn build_hook_dispatcher(plugin_host: &Arc<Mutex<PluginHost>>) -> HookDispatcher {
    let mut dispatcher = HookDispatcher::new();
    for h in arbor_plugin_types::prelude::HOOK_CATALOG {
        dispatcher.register_hook(HookDef {
            name:        h.name,
            category:    h.category,
            description: h.description,
            kind:        if h.name == "on_pre_commit" {
                HookKind::Vetoable
            } else {
                HookKind::FireAndForget
            },
            ctx: h.ctx,
        });
    }
    dispatcher.register_listener(Arc::new(LuaHookListener::new(Arc::downgrade(plugin_host))));
    dispatcher
}
