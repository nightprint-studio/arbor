//! Settings scope (`arbor.settings.global` / `arbor.settings.project`).
//!
//! Builds a Lua table with `get / set / get_all / clear` backed by an
//! in-memory cache plus on-disk persistence (delegated to
//! `plugin::settings_store`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::{Lua, LuaSerdeExt, Table};

use crate::plugin::settings_store::{
    SettingsMap, global_settings_path, project_settings_path,
    load_settings_file, save_settings_file,
};

use super::convert::{json_to_lua, lua_value_to_json};
use super::fs_perm::current_repo;

pub(crate) type GlobalCache  = Arc<Mutex<Option<SettingsMap>>>;
pub(crate) type ProjectCache = Arc<Mutex<HashMap<String, SettingsMap>>>;

pub(crate) fn build_settings_scope(
    lua: &Lua,
    plugin_name: String,
    global_cache: GlobalCache,
    project_cache: ProjectCache,
    scope: &'static str, // "global" or "project"
) -> mlua::Result<Table> {
    let scope_table = lua.create_table()?;
    let pname = plugin_name.clone();

    // ── get(key) → value | nil ──────────────────────────────────────────
    {
        let pname   = pname.clone();
        let gc      = global_cache.clone();
        let pc      = project_cache.clone();
        let get_fn  = lua.create_function(move |lua_ctx, key: String| {
            let map = get_map(scope, &pname, &gc, &pc, lua_ctx)?;
            match map.get(&key) {
                None => Ok(mlua::Value::Nil),
                Some(v) => json_to_lua(lua_ctx, v).map_err(|e| mlua::Error::RuntimeError(e.to_string())),
            }
        })?;
        scope_table.set("get", get_fn)?;
    }

    // ── set(key, value) ─────────────────────────────────────────────────
    {
        let pname   = pname.clone();
        let gc      = global_cache.clone();
        let pc      = project_cache.clone();
        let set_fn  = lua.create_function(move |lua_ctx, (key, value): (String, mlua::Value)| {
            let mut map = get_map(scope, &pname, &gc, &pc, lua_ctx)?;
            match value {
                mlua::Value::Nil => { map.remove(&key); }
                mlua::Value::Table(t) => {
                    let actual: serde_json::Value = lua_ctx
                        .from_value(mlua::Value::Table(t))
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                    map.insert(key, actual);
                }
                other => {
                    if let Some(json_val) = lua_value_to_json(other) {
                        map.insert(key, json_val);
                    }
                }
            }
            flush_map(scope, &pname, &gc, &pc, lua_ctx, map)?;
            Ok(())
        })?;
        scope_table.set("set", set_fn)?;
    }

    // ── get_all() → table ───────────────────────────────────────────────
    {
        let pname  = pname.clone();
        let gc     = global_cache.clone();
        let pc     = project_cache.clone();
        let all_fn = lua.create_function(move |lua_ctx, ()| {
            let map = get_map(scope, &pname, &gc, &pc, lua_ctx)?;
            let json = serde_json::Value::Object(map);
            json_to_lua(lua_ctx, &json)
        })?;
        scope_table.set("get_all", all_fn)?;
    }

    // ── clear(key) ──────────────────────────────────────────────────────
    {
        let pname     = pname.clone();
        let gc        = global_cache.clone();
        let pc        = project_cache.clone();
        let clear_fn  = lua.create_function(move |lua_ctx, key: String| {
            let mut map = get_map(scope, &pname, &gc, &pc, lua_ctx)?;
            map.remove(&key);
            flush_map(scope, &pname, &gc, &pc, lua_ctx, map)?;
            Ok(())
        })?;
        scope_table.set("clear", clear_fn)?;
    }

    Ok(scope_table)
}

/// Load (or return cached) settings for the given scope.
fn get_map(
    scope: &str,
    plugin_name: &str,
    gc: &GlobalCache,
    pc: &ProjectCache,
    lua_ctx: &Lua,
) -> mlua::Result<SettingsMap> {
    if scope == "global" {
        let mut lock = gc.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if lock.is_none() {
            *lock = Some(load_settings_file(&global_settings_path(plugin_name)));
        }
        Ok(lock.as_ref().expect("global settings cache is Some after init").clone())
    } else {
        let repo = current_repo(lua_ctx)?;
        let mut lock = pc.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if !lock.contains_key(&repo) {
            lock.insert(repo.clone(), load_settings_file(&project_settings_path(plugin_name, &repo)));
        }
        Ok(lock.get(&repo).expect("project settings cache has entry after insert").clone())
    }
}

/// Write back a modified map and flush to disk.
fn flush_map(
    scope: &str,
    plugin_name: &str,
    gc: &GlobalCache,
    pc: &ProjectCache,
    lua_ctx: &Lua,
    map: SettingsMap,
) -> mlua::Result<()> {
    if scope == "global" {
        let path = global_settings_path(plugin_name);
        save_settings_file(&path, &map);
        let mut lock = gc.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        *lock = Some(map);
    } else {
        let repo = current_repo(lua_ctx)?;
        let path = project_settings_path(plugin_name, &repo);
        save_settings_file(&path, &map);
        let mut lock = pc.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        lock.insert(repo, map);
    }
    Ok(())
}
