//! `arbor.meta` — plugin / runtime introspection.

use mlua::{Lua, Table};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;
use crate::runtime::ARBOR_APP_VERSION;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let meta_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    let pname    = ctx.plugin_name.clone();
    let pdir     = ctx.plugin_dir.clone();
    let api_ver  = ctx.arbor_api;
    let app_ctx  = ctx.app_ctx.clone();
    let host     = ctx.host_weak.clone();

    let pn = pname.clone();
    meta_table.set("plugin_name", lua.create_function(move |lua_ctx, ()| {
        Ok(lua_ctx.create_string(pn.as_bytes())?)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // is_app_focused() → bool. Reads the same flag the scheduler uses
    // to gate `only_when_focused` ticks. Plugins that drive their own
    // `arbor.timer.every` polls can call this to skip work while the
    // user is alt-tabbed away — the bigger payoff is on focus-regain,
    // since otherwise the OS efficiency-mode backlog re-bursts in one
    // shot the moment the window comes back.
    let app_ctx_focus = app_ctx.clone();
    meta_table.set("is_app_focused", lua.create_function(move |_, ()| {
        Ok(app_ctx_focus.as_ref().map(|c| c.is_focused()).unwrap_or(true))
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    meta_table.set("api_version", lua.create_function(move |_, ()| {
        Ok(api_ver)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    meta_table.set("app_version", lua.create_function(|lua_ctx, ()| {
        Ok(lua_ctx.create_string(ARBOR_APP_VERSION.as_bytes())?)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let dir_str = pdir.to_string_lossy().into_owned();
    meta_table.set("plugin_dir", lua.create_function(move |lua_ctx, ()| {
        Ok(lua_ctx.create_string(dir_str.as_bytes())?)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // os() → "windows" | "macos" | "linux"
    meta_table.set("os", lua.create_function(|lua_ctx, ()| {
        let name = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        };
        Ok(lua_ctx.create_string(name.as_bytes())?)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // plugin_loaded(name) → bool. True when a plugin with that manifest
    // name is currently loaded AND enabled. Used by sibling plugins that
    // need to decide their behaviour based on whether another plugin is
    // active right now (e.g. run-action checks for run-monitor to decide
    // whether to spawn Services jobs as hidden). Synchronous: reads the
    // host's plugin list under the plugin_host mutex. Returns false on
    // any lookup failure so callers can chain it as a soft check.
    meta_table.set("plugin_loaded", lua.create_function(move |_, name: String| {
        let loaded = host.as_ref()
            .and_then(|w| w.upgrade())
            .and_then(|arc| arc.lock().ok().map(|h| h.is_plugin_enabled(&name)))
            .unwrap_or(false);
        Ok(loaded)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    arbor.set("meta", meta_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
