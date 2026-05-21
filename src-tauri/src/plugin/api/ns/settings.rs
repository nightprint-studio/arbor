//! `arbor.settings.global` / `arbor.settings.project` plus cross-plugin
//! readers (`arbor.settings.read`, `arbor.settings.read_project`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::convert::json_to_lua;
use crate::plugin::api::helpers::settings_scope::{
    GlobalCache, ProjectCache, build_settings_scope,
};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let global_cache:  GlobalCache  = Arc::new(Mutex::new(None));
    let project_cache: ProjectCache = Arc::new(Mutex::new(HashMap::new()));

    let settings_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    let global_scope = build_settings_scope(
        lua, ctx.plugin_name.clone(), global_cache.clone(), project_cache.clone(), "global",
    ).map_err(|e| AppError::Plugin(e.to_string()))?;

    let project_scope = build_settings_scope(
        lua, ctx.plugin_name.clone(), global_cache, project_cache, "project",
    ).map_err(|e| AppError::Plugin(e.to_string()))?;

    settings_table.set("global",  global_scope).map_err(|e| AppError::Plugin(e.to_string()))?;
    settings_table.set("project", project_scope).map_err(|e| AppError::Plugin(e.to_string()))?;

    install_read(ctx, lua, &settings_table)?;
    install_read_project(ctx, lua, &settings_table)?;

    arbor.set("settings", settings_table).map_err(|e| AppError::Plugin(e.to_string()))?;
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
        let path = crate::plugin::settings_store::global_settings_path(&target_plugin);
        let map  = crate::plugin::settings_store::load_settings_file(&path);
        match map.get(&key) {
            Some(v) => json_to_lua(lua_ctx, v),
            None    => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    settings_table.set("read", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_read_project(ctx: &ApiCtx, lua: &Lua, settings_table: &Table) -> Result<()> {
    // arbor.settings.read_project(plugin_name, key) — same, but for the
    // project (per-repo) scope. Resolves the active repo's path from
    // AppState; returns nil if there is no active tab.
    let pname = ctx.plugin_name.clone();
    let allow_others = ctx.settings_read_others;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, (target_plugin, key): (String, String)| {
        if target_plugin != pname && !allow_others {
            return Err(mlua::Error::RuntimeError(
                "arbor.settings.read_project: settings_read_others permission required".to_string()
            ));
        }
        let Some(ref handle) = h else { return Ok(mlua::Value::Nil) };
        let state = handle.state::<crate::AppState>();
        let active_tab = state.active_tab_id.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
            .clone();
        let Some(tab_id) = active_tab else { return Ok(mlua::Value::Nil) };
        let mut repos = state.repos.lock()
            .map_err(|_| mlua::Error::RuntimeError("repos mutex poisoned".to_string()))?;
        let Ok(repo) = repos.get(&tab_id) else { return Ok(mlua::Value::Nil) };
        let repo_path = repo.path.clone();
        drop(repos);
        let path = crate::plugin::settings_store::project_settings_path(&target_plugin, &repo_path);
        let map  = crate::plugin::settings_store::load_settings_file(&path);
        match map.get(&key) {
            Some(v) => json_to_lua(lua_ctx, v),
            None    => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    settings_table.set("read_project", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
