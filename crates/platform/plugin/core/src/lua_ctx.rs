//! Lua-side handle to the host context (plugin name + [`AppCtx`]).
//!
//! Stashed in each plugin's Lua VM via `lua.set_app_data` when the sandbox is
//! built, so code paths that only have access to a `&Lua` (hook handler
//! dispatch, service-callback delivery) can surface runtime errors to the
//! Plugin Logs panel without threading extra arguments through their
//! signatures.
//!
//! This is the `&Lua`-bound door onto [`crate::report::PluginReporter`]. Use
//! [`report`] rather than [`record`] unless the site already logged the same
//! message itself: `report` writes it once and it reaches both the console and
//! the panel, which is the pairing every plugin failure owes its reader.

use std::sync::Arc;

use arbor_core::prelude::AppCtx;
use mlua::Lua;

use crate::report::PluginReporter;

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
///
/// Panel only. A site that has not already logged the message itself wants
/// [`report`] instead — a failure that reaches one audience and not the other
/// is the thing this module exists to stop.
pub fn record(lua: &Lua, level: &str, message: String) {
    let Some(ctx) = lua.app_data_ref::<PluginLuaCtx>() else { return; };
    let Some(app_ctx) = ctx.app_ctx.as_ref() else { return; };
    app_ctx.record_plugin_log(level, &ctx.plugin_name, &message);
}

/// The owning plugin's [`PluginReporter`], cloned out of the VM.
///
/// Cloned rather than borrowed because the interesting callers are closures and
/// spawned threads, and a `&Lua` crosses neither. `None` for a VM with no
/// [`PluginLuaCtx`] attached.
pub fn reporter(lua: &Lua) -> Option<PluginReporter> {
    let ctx = lua.app_data_ref::<PluginLuaCtx>()?;
    Some(PluginReporter::new(ctx.plugin_name.clone(), ctx.app_ctx.clone()))
}

/// Say it once, to both audiences: the dev console and the Plugin Logs panel.
///
/// The `&Lua`-bound form of [`PluginReporter::error`] and friends, for the
/// dispatch paths that hold a VM and nothing else. No-op on a VM with no
/// [`PluginLuaCtx`], which is what a unit test has.
pub fn report(lua: &Lua, level: &str, message: impl Into<String>) {
    let Some(r) = reporter(lua) else { return; };
    match level {
        "error" => r.error(message),
        "warn" => r.warn(message),
        _ => r.info(message),
    }
}
