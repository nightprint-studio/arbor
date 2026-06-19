//! Headless `arbor.*` installer for `corvus-be`.

use std::sync::Arc;

use arbor_plugin_core::prelude::{
    register_lua_api, ApiInstallParams, LuaApiInstaller, LuaNamespaceInstaller, PluginCoreResult,
};
use mlua::Lua;

/// Publishes only the **host-pure** `arbor.*` namespaces (log, events, json,
/// text, fs, http, meta, settings, timer, scheduler, hooks, contribution,
/// keybinding, command, notify, the `*_studio`s, …) that `register_lua_api`
/// hardcodes — passing **no** extra namespace installers.
///
/// The git/product `ns_shell` namespaces (`arbor.repo`, `arbor.job`,
/// `arbor.pipeline`, …) are not wired here yet: they move into the backend
/// next to their domain logic in plugin-relocation Wave 1. Until then a plugin
/// that calls one in a hook running OOP gets a clear nil-field error, logged by
/// the host — never a silent drop.
pub struct CorvusBeApiInstaller;

impl LuaApiInstaller for CorvusBeApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        let extra: Vec<Arc<dyn LuaNamespaceInstaller>> = Vec::new();
        register_lua_api(lua, params, &extra)
    }
}

/// Convenience constructor so `corvus-be` wires the installer without naming
/// `mlua` itself.
pub fn corvus_be_api_installer() -> Arc<dyn LuaApiInstaller> {
    Arc::new(CorvusBeApiInstaller)
}
