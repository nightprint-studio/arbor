//! Build the runtime hook dispatcher for a Corvus plugin host.

use std::sync::{Arc, Mutex};

use arbor_plugin_api::prelude::{HookDef, HookDispatcher, HookKind};
use arbor_plugin_core::prelude::{LuaHookListener, PluginHost};

/// Build the [`HookDispatcher`] for a Corvus plugin host: register every hook in
/// the static catalog (marking `on_pre_commit` vetoable, the rest
/// fire-and-forget) and bind a single [`LuaHookListener`] to `plugin_host`.
///
/// The shell's in-process host and the headless `corvus-be` host build their
/// dispatcher through this one function, so a fire from either side fans out
/// identically — the listener walks the same `PluginHost` shape in both.
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
