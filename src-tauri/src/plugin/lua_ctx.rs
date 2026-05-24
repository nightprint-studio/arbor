//! Lua-side handle to the host context (plugin name + AppHandle).
//!
//! Stashed in each plugin's Lua VM via `lua.set_app_data` when the sandbox is
//! built, so code paths that only have access to a `&Lua` (hook handler
//! dispatch, service-callback delivery) can surface runtime errors to the
//! Plugin Logs panel without threading extra arguments through their
//! signatures.

use mlua::Lua;
use tauri::AppHandle;

#[derive(Clone)]
pub struct PluginLuaCtx {
    pub plugin_name: String,
    pub app_handle:  Option<AppHandle>,
}

pub fn install(lua: &Lua, plugin_name: String, app_handle: Option<AppHandle>) {
    lua.set_app_data(PluginLuaCtx { plugin_name, app_handle });
}

/// Record a message in the Plugin Logs panel attributed to the owning plugin.
/// No-op when the Lua state has no `PluginLuaCtx` attached or the app handle
/// is unavailable (test runs, headless calls).
pub fn record(lua: &Lua, level: &str, message: String) {
    let Some(ctx) = lua.app_data_ref::<PluginLuaCtx>() else { return; };
    let Some(ref h) = ctx.app_handle else { return; };
    crate::plugin_logs::record(h, level, &ctx.plugin_name, message);
}
