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

    let pn = pname.clone();
    meta_table.set("plugin_name", lua.create_function(move |lua_ctx, ()| {
        lua_ctx.create_string(pn.as_bytes())
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
        lua_ctx.create_string(ARBOR_APP_VERSION.as_bytes())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let dir_str = pdir.to_string_lossy().into_owned();
    meta_table.set("plugin_dir", lua.create_function(move |lua_ctx, ()| {
        lua_ctx.create_string(dir_str.as_bytes())
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
        lua_ctx.create_string(name.as_bytes())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // plugin_loaded(name) → bool. True when a plugin with that manifest
    // name is currently loaded AND enabled. Used by sibling plugins that
    // need to decide their behaviour based on whether another plugin is
    // active right now (e.g. run-action checks for run-monitor to decide
    // whether to spawn Services jobs as hidden). Returns false on any
    // lookup failure so callers can chain it as a soft check.
    //
    // Answered from `PluginActivity`, NOT by locking the host. The moment a plugin most
    // wants to ask this is its own `arbor:plugin_load` hook — and the host fires that hook
    // while holding its own mutex, so locking here re-entered a `std::sync::Mutex` on the
    // same thread and hung the backend outright. See `PluginActivity` for the full story;
    // the short version is that any *synchronous* host lock reachable from Lua is a
    // deadlock waiting for a plugin to call it from a hook.
    let activity = ctx.activity.clone();
    meta_table.set("plugin_loaded", lua.create_function(move |_, name: String| {
        Ok(activity.is_enabled(&name))
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    arbor.set("meta", meta_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
