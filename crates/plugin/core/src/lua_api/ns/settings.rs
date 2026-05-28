//! `arbor.settings.global` / `arbor.settings.project` plus cross-plugin
//! readers (`arbor.settings.read`, `arbor.settings.read_project`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;
use crate::lua_api::helpers::convert::json_to_lua;
use crate::lua_api::helpers::settings_scope::{
    GlobalCache, ProjectCache, build_settings_scope,
};
use crate::settings_store::{global_settings_path, load_settings_file, project_settings_path};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let global_cache:  GlobalCache  = Arc::new(Mutex::new(None));
    let project_cache: ProjectCache = Arc::new(Mutex::new(HashMap::new()));

    let settings_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let global_scope = build_settings_scope(
        lua, ctx.plugin_name.clone(), global_cache.clone(), project_cache.clone(), "global",
    ).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let project_scope = build_settings_scope(
        lua, ctx.plugin_name.clone(), global_cache, project_cache, "project",
    ).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    settings_table.set("global",  global_scope).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    settings_table.set("project", project_scope).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    install_read(ctx, lua, &settings_table)?;
    install_read_project(ctx, lua, &settings_table)?;

    arbor.set("settings", settings_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_read(ctx: &ApiCtx, lua: &Lua, settings_table: &Table) -> Result<()> {
    // arbor.settings.read(plugin_name, key) → value | nil
    //
    // Cross-plugin read into another plugin's `global.json`. Always allowed
    // for self-reads; reads against any other plugin require the
    // `settings_read_others` permission. Cross-plugin WRITE is not exposed
    // here — the target plugin must opt in via `arbor.service.export` and
    // the caller invokes it through `arbor.service.call`.
    let pname = ctx.plugin_name.clone();
    let allow_others = ctx.settings_read_others;
    let fn_ = lua.create_function(move |lua_ctx, (target_plugin, key): (String, String)| {
        if target_plugin != pname && !allow_others {
            return Err(mlua::Error::RuntimeError(
                "arbor.settings.read: settings_read_others permission required to read other plugins' settings".to_string()
            ));
        }
        let path = global_settings_path(&target_plugin);
        let map  = load_settings_file(&path);
        match map.get(&key) {
            Some(v) => json_to_lua(lua_ctx, v),
            None    => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    settings_table.set("read", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_read_project(ctx: &ApiCtx, lua: &Lua, settings_table: &Table) -> Result<()> {
    // arbor.settings.read_project(plugin_name, key) — same, but for the
    // project (per-repo) scope. Resolves the active repo's path from the
    // host capability handle; returns nil if there is no active tab.
    let pname = ctx.plugin_name.clone();
    let allow_others = ctx.settings_read_others;
    let app_ctx = ctx.app_ctx.clone();
    let fn_ = lua.create_function(move |lua_ctx, (target_plugin, key): (String, String)| {
        if target_plugin != pname && !allow_others {
            return Err(mlua::Error::RuntimeError(
                "arbor.settings.read_project: settings_read_others permission required".to_string()
            ));
        }
        let Some(repo_path) = app_ctx.as_ref().and_then(|c| c.active_repo_path()) else {
            return Ok(mlua::Value::Nil);
        };
        let path = project_settings_path(&target_plugin, &repo_path.to_string_lossy());
        let map  = load_settings_file(&path);
        match map.get(&key) {
            Some(v) => json_to_lua(lua_ctx, v),
            None    => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    settings_table.set("read_project", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
