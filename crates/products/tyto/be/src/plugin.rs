//! Minimal plugin-host wiring for tyto-be.
//!
//! tyto-be loads **host-pure** Lua plugins (the `arbor.*` namespaces that don't
//! touch a product domain: log/events/json/text/fs/http/meta/settings/timer/
//! scheduler/hooks/contribution/keybinding/command/notify/the studios) exactly the
//! way sitta-be loads them — but with **no product namespaces** and **no vetoable
//! hooks**.
//!
//! Identical shape to `sitta-be/src/plugin.rs`. If a third product wants the same
//! host-pure wiring, promote this to a shared `arbor-plugin-be` crate rather than
//! copying it a fourth time.
//!
//! NOTE: this wires the host so plugins *load + run* (hooks fire, contributions
//! flush). The recorder lifecycle hooks (`on_recording_started`, …) are added to
//! the shared `HOOK_CATALOG` when the recording engine actually fires them — the
//! dispatcher below picks them up automatically once they're in the catalog. The
//! Plugin-Manager RPC surface (FE enable/disable/reload) is intentionally not
//! wired yet — there are no tyto plugins to manage.

use std::sync::{Arc, Mutex};

use arbor_plugin_api::prelude::{HookDef, HookDispatcher, HookKind};
use arbor_plugin_core::prelude::{
    register_lua_api, ApiInstallParams, LuaApiInstaller, LuaHookListener, LuaNamespaceInstaller,
    PluginCoreResult, PluginHost,
};
use mlua::Lua;

/// Build the [`HookDispatcher`] for tyto's plugin host: register every hook in the
/// shared catalog (all fire-and-forget — tyto fires no vetoable hooks) and bind a
/// single [`LuaHookListener`] to `plugin_host`. Hooks that don't apply to tyto
/// simply never fire; a tyto plugin can still listen on lifecycle hooks
/// (`on_plugin_load`, …).
pub fn tyto_hook_dispatcher(plugin_host: &Arc<Mutex<PluginHost>>) -> HookDispatcher {
    let mut dispatcher = HookDispatcher::new();
    for h in arbor_plugin_types::prelude::HOOK_CATALOG {
        dispatcher.register_hook(HookDef {
            name:        h.name,
            category:    h.category,
            description: h.description,
            kind:        HookKind::FireAndForget,
            ctx:         h.ctx,
        });
    }
    dispatcher.register_listener(Arc::new(LuaHookListener::new(Arc::downgrade(plugin_host))));
    dispatcher
}

/// Publishes only the **host-pure** `arbor.*` namespaces that `register_lua_api`
/// hardcodes — no product namespaces (the `extra` list is empty).
pub struct TytoBeApiInstaller;

impl LuaApiInstaller for TytoBeApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        let no_extra: Vec<Arc<dyn LuaNamespaceInstaller>> = Vec::new();
        register_lua_api(lua, params, &no_extra)
    }
}

/// Convenience constructor so `main` wires the installer without naming `mlua`.
pub fn tyto_be_api_installer() -> Arc<dyn LuaApiInstaller> {
    Arc::new(TytoBeApiInstaller)
}
