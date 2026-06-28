//! Headless `arbor.*` installer for `corvus-be`.

use std::sync::Arc;

use arbor_plugin_core::prelude::{
    register_lua_api, ApiInstallParams, LuaApiInstaller, LuaNamespaceInstaller, PluginCoreResult,
};
use mlua::Lua;

/// Publishes the **host-pure** `arbor.*` namespaces (log, events, json, text, fs,
/// http, meta, settings, timer, scheduler, hooks, contribution, keybinding,
/// command, notify, the `*_studio`s, …) that `register_lua_api` hardcodes, plus
/// any **git/product `ns_shell` namespaces** the backend hands in (`extra`).
///
/// The git namespaces (`arbor.notes`, `arbor.repo`, …) are ported into
/// `corvus-plugin-ns` and built by `corvus-be` next to their `NsHost` impl, then
/// passed here. Whatever the backend doesn't supply yet stays absent — a plugin
/// that calls a not-yet-wired namespace in an OOP hook gets a clear nil-field
/// error, logged by the host, never a silent drop.
pub struct CorvusBeApiInstaller {
    /// The backend-supplied git/product namespace installers, run (in order)
    /// after the host-pure namespaces — exactly the slot the Tauri shell passes
    /// its `ns_shell` wrappers into.
    extra: Vec<Arc<dyn LuaNamespaceInstaller>>,
}

impl LuaApiInstaller for CorvusBeApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        register_lua_api(lua, params, &self.extra)
    }
}

/// Convenience constructor so `corvus-be` wires the installer without naming
/// `mlua` itself. `extra` is the ordered list of git/product namespace installers
/// (built in `corvus-be` over its `CorvusNsHost`); pass an empty `Vec` for a
/// host-pure-only surface.
pub fn corvus_be_api_installer(
    extra: Vec<Arc<dyn LuaNamespaceInstaller>>,
) -> Arc<dyn LuaApiInstaller> {
    Arc::new(CorvusBeApiInstaller { extra })
}
