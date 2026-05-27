//! `LuaApiInstaller` adapter that bridges the runtime-side
//! `arbor-plugin-core::sandbox` builder back into the shell-crate-resident
//! `arbor.*` namespace surface.
//!
//! After PR #4 Step 5, the orchestrator (`register(...)`) lives in
//! `arbor_plugin_core::lua_api`. This adapter:
//!   1. Builds the ordered list of [`LuaNamespaceInstaller`] wrappers for
//!      each ns/* that still lives in `src-tauri/src/plugin/api/ns/*`
//!      (see [`crate::plugin::api::shell_installers`]).
//!   2. Hands the parameter bag straight through to
//!      [`arbor_plugin_core::prelude::register_lua_api`].
//!
//! Once every ns/* migrates into its own domain crate (PR #6+), the
//! wrapper list shrinks to empty and this adapter (and the `LuaApiInstaller`
//! trait it implements) can disappear.

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
        let installers = crate::plugin::api::shell_installers();
        register_lua_api(lua, params, &installers)
    }
}

/// Convenience wrapper so the boot site can write
/// `host.set_api_installer(tauri_api_installer())`.
pub fn tauri_api_installer() -> Arc<dyn LuaApiInstaller> {
    Arc::new(TauriApiInstaller::new())
}
