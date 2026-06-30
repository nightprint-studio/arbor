//! Lua-side handle to the host context (plugin name + [`AppCtx`]).
//!
//! Stashed in each plugin's Lua VM via `lua.set_app_data` when the sandbox is
//! built, so code paths that only have access to a `&Lua` (hook handler
//! dispatch, service-callback delivery) can surface runtime errors to the
//! Plugin Logs panel without threading extra arguments through their
//! signatures.

use std::sync::Arc;

use arbor_core::prelude::AppCtx;
use mlua::Lua;

/// Per-VM bundle stashed under [`Lua::set_app_data`].
///
/// `app_ctx` is optional so headless / test setups can install a Lua VM
/// without an attached host (in which case [`record`] becomes a no-op).
#[derive(Clone)]
pub struct PluginLuaCtx {
    pub plugin_name: String,
    pub app_ctx:     Option<Arc<dyn AppCtx>>,
}

/// Attach a [`PluginLuaCtx`] to the Lua VM. Called once per plugin during
/// sandbox creation.
pub fn install(lua: &Lua, plugin_name: String, app_ctx: Option<Arc<dyn AppCtx>>) {
    lua.set_app_data(PluginLuaCtx { plugin_name, app_ctx });
}

/// Record a message in the Plugin Logs panel attributed to the owning plugin.
/// No-op when the Lua state has no [`PluginLuaCtx`] attached or the host
/// context is unavailable (test runs, headless calls).
pub fn record(lua: &Lua, level: &str, message: String) {
    let Some(ctx) = lua.app_data_ref::<PluginLuaCtx>() else { return; };
    let Some(app_ctx) = ctx.app_ctx.as_ref() else { return; };
    app_ctx.record_plugin_log(level, &ctx.plugin_name, &message);
}
