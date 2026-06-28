//! `LuaApiInstaller` adapter for the **launcher** host.
//!
//! The git `arbor.*` namespaces (`repo`, `workspace`, `mr`, `pipeline`, …)
//! relocated to `corvus-be` (crate `corvus-plugin-ns`) with the plugin
//! product-relocation flip — `corvus-be` is the sole loader of the git
//! product's plugins. The launcher (this shell) loads only plugins that target
//! `launcher`, so it publishes just the host-pure base surface that
//! `arbor_plugin_core::lua_api::register` hardcodes (`log`, `events`, `json`,
//! `fs`, `http`, `ui.*`, …) and **no** product-specific extras.
//!
//! This adapter therefore hands an empty extras list straight through to
//! [`arbor_plugin_core::prelude::register_lua_api`]. When a launcher-scoped
//! namespace is ever needed, add its installer to the slice below.

use std::sync::Arc;

use arbor_plugin_core::prelude::{
    ApiInstallParams, LuaApiInstaller, PluginCoreResult, register_lua_api,
};
use mlua::Lua;

pub struct TauriApiInstaller;

impl TauriApiInstaller {
    pub fn new() -> Self { Self }
}

impl Default for TauriApiInstaller {
    fn default() -> Self { Self::new() }
}

impl LuaApiInstaller for TauriApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        // Host-pure base only — no launcher-specific namespaces today.
        register_lua_api(lua, params, &[])
    }
}

/// Convenience wrapper so the boot site can write
/// `host.set_api_installer(tauri_api_installer())`.
pub fn tauri_api_installer() -> Arc<dyn LuaApiInstaller> {
    Arc::new(TauriApiInstaller::new())
}
