//! `LuaApiInstaller` adapter that bridges the runtime-side
//! `arbor-plugin-core::sandbox` builder back into the shell-crate-resident
//! `arbor.*` namespace surface (`crate::plugin::api::register`).
//!
//! The src-tauri shell still owns the per-namespace installers (because they
//! pull on git/jobs/pipeline/cloud/… internal types that haven't migrated to
//! their own crates yet). This struct is the single touch-point between the
//! plugin-core sandbox builder and that surface — once steps 5-7 of PR #4
//! atterrano, the trait is implemented in the `lua_api` module of
//! `arbor-plugin-core` and this shim disappears.

use std::sync::Arc;

use arbor_plugin_core::error::PluginCoreError;
use arbor_plugin_core::prelude::{ApiInstallParams, LuaApiInstaller, PluginCoreResult};
use mlua::Lua;

/// Wraps the Tauri `AppHandle` captured at boot so the plugin-host crate can
/// publish `arbor.*` into a freshly-built sandbox without ever seeing a
/// concrete `tauri::*` type itself.
pub struct TauriApiInstaller {
    app_handle: Option<tauri::AppHandle>,
}

impl TauriApiInstaller {
    pub fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        Self { app_handle }
    }
}

impl LuaApiInstaller for TauriApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        crate::plugin::api::register(
            lua,
            params.plugin_name,
            params.plugin_dir,
            params.arbor_api,
            self.app_handle.clone(),
            params.timer_cancels,
            params.timer_counter,
            params.schedules,
            params.scheduler_enabled,
            params.permissions,
            params.contributions,
            params.tree_store,
            params.icon_registry,
            params.enabled,
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        // Capture the AppCtx so `_unused` compiles when no body reads it.
        // The host side already routes through `params.app_ctx` for the
        // log/event surface; the legacy `register(...)` path uses the
        // AppHandle stashed on this struct instead.
        let _ = params.app_ctx;
        Ok(())
    }
}

/// Convenience wrapper so the boot site can write
/// `host.set_api_installer(tauri_api_installer(handle))`.
pub fn tauri_api_installer(handle: Option<tauri::AppHandle>) -> Arc<dyn LuaApiInstaller> {
    Arc::new(TauriApiInstaller::new(handle))
}
