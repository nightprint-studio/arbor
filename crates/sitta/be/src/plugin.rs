//! Minimal plugin-host wiring for sitta-be.
//!
//! sitta-be loads **host-pure** Lua plugins (the `arbor.*` namespaces that don't
//! touch a product domain: log/events/json/text/fs/http/meta/settings/timer/
//! scheduler/hooks/contribution/keybinding/command/notify/the studios) exactly the
//! way corvus-be loads the git product's — but with **no product namespaces** (the
//! file explorer contributes none yet) and **no vetoable hooks**.
//!
//! Mirrors `corvus-plugin`'s `build_hook_dispatcher` + `CorvusBeApiInstaller`, but
//! kept local so sitta doesn't depend on the *git* product's plugin crate. If a
//! third product wants the same host-pure wiring, promote this to a shared
//! `arbor-plugin-be` crate rather than copying it a third time.
//!
//! NOTE: this wires the host so plugins *load + run* (hooks fire, contributions
//! flush). The Plugin-Manager RPC surface (FE enable/disable/reload for sitta
//! plugins) is intentionally not wired yet — there are no sitta plugins to manage.

use std::sync::{Arc, Mutex};

use arbor_plugin_api::prelude::{HookDef, HookDispatcher, HookKind};
use arbor_plugin_core::prelude::{
    register_lua_api, ApiInstallParams, LuaApiInstaller, LuaHookListener, LuaNamespaceInstaller,
    PluginCoreResult, PluginHost,
};
use mlua::Lua;

/// Build the [`HookDispatcher`] for sitta's plugin host: register every hook in
/// the shared catalog (all fire-and-forget — sitta fires no vetoable hooks) and
/// bind a single [`LuaHookListener`] to `plugin_host`. The git/repo hooks in the
/// catalog simply never fire here; a sitta plugin can still listen on lifecycle
/// hooks (`on_plugin_load`, …).
pub fn sitta_hook_dispatcher(plugin_host: &Arc<Mutex<PluginHost>>) -> HookDispatcher {
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
pub struct SittaBeApiInstaller;

impl LuaApiInstaller for SittaBeApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        let no_extra: Vec<Arc<dyn LuaNamespaceInstaller>> = Vec::new();
        register_lua_api(lua, params, &no_extra)
    }
}

/// Convenience constructor so `main` wires the installer without naming `mlua`.
pub fn sitta_be_api_installer() -> Arc<dyn LuaApiInstaller> {
    Arc::new(SittaBeApiInstaller)
}
