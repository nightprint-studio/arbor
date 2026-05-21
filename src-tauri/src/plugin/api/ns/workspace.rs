//! `arbor.workspace` — workspace / repo-registry queries.
//!
//! Surface kept minimal and read-only in v1: plugins can list workspaces,
//! inspect membership, resolve repo metadata, and trigger a switch.
//! Mutation beyond switch lives in Arbor's Tauri commands for now to
//! avoid granting plugins carte blanche over the user's workspace layout.

use mlua::{Lua, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let ws_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(ctx, lua, &ws_table)?;
    install_active(ctx, lua, &ws_table)?;
    install_get(ctx, lua, &ws_table)?;
    install_list_repos(ctx, lua, &ws_table)?;
    install_repo(ctx, lua, &ws_table)?;
    install_switch(ctx, lua, &ws_table)?;

    arbor.set("workspace", ws_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── Lua-table conversion helpers ────────────────────────────────────────

fn ws_to_lua(lua: &Lua, ws: &crate::workspace::WorkspaceDef) -> mlua::Result<mlua::Value> {
    let t = lua.create_table()?;
    t.set("id",        ws.id.as_str())?;
    t.set("name",      ws.name.as_str())?;
    t.set("color_idx", ws.color_idx)?;
    t.set("group_id",  ws.group_id.clone())?;
    let ids = lua.create_table()?;
    for (i, id) in ws.repo_ids.iter().enumerate() {
        ids.set(i + 1, id.as_str())?;
    }
    t.set("repo_ids", ids)?;
    t.set("repo_count", ws.repo_ids.len() as i64)?;
    Ok(mlua::Value::Table(t))
}

fn entry_to_lua(lua: &Lua, e: &crate::workspace::RepoRegistryEntry) -> mlua::Result<mlua::Value> {
    let t = lua.create_table()?;
    t.set("id",           e.id.as_str())?;
    t.set("path",         e.path.as_str())?;
    t.set("display_name", e.display_name.as_str())?;
    t.set("remote_url",   e.remote_url.clone())?;
    Ok(mlua::Value::Table(t))
}

// ─── Functions ───────────────────────────────────────────────────────────

fn install_list(ctx: &ApiCtx, lua: &Lua, ws_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ()| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let store = state.workspaces.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let arr = lua_ctx.create_table()?;
        for (i, ws) in store.ordered().iter().enumerate() {
            arr.set(i + 1, ws_to_lua(lua_ctx, ws)?)?;
        }
        Ok(mlua::Value::Table(arr))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ws_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_active(ctx: &ApiCtx, lua: &Lua, ws_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ()| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let store = state.workspaces.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match store.active() {
            Some(ws) => ws_to_lua(lua_ctx, ws),
            None     => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ws_table.set("active", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(ctx: &ApiCtx, lua: &Lua, ws_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ws_id: String| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let store = state.workspaces.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match store.get(&ws_id) {
            Some(ws) => ws_to_lua(lua_ctx, ws),
            None     => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ws_table.set("get", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_repos(ctx: &ApiCtx, lua: &Lua, ws_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ws_id: Option<String>| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let reg = state.repo_registry.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let arr = lua_ctx.create_table()?;
        match ws_id {
            Some(id) => {
                let store = state.workspaces.lock()
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                let ws = match store.get(&id) {
                    Some(w) => w,
                    None => return Ok(mlua::Value::Table(arr)),
                };
                let mut i = 1;
                for repo_id in &ws.repo_ids {
                    if let Some(e) = reg.get(repo_id) {
                        arr.set(i, entry_to_lua(lua_ctx, e)?)?;
                        i += 1;
                    }
                }
            }
            None => {
                for (i, e) in reg.list().iter().enumerate() {
                    arr.set(i + 1, entry_to_lua(lua_ctx, e)?)?;
                }
            }
        }
        Ok(mlua::Value::Table(arr))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ws_table.set("list_repos", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_repo(ctx: &ApiCtx, lua: &Lua, ws_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, repo_id: String| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let reg = state.repo_registry.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match reg.get(&repo_id) {
            Some(e) => entry_to_lua(lua_ctx, e),
            None    => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ws_table.set("repo", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_switch(ctx: &ApiCtx, lua: &Lua, ws_table: &Table) -> Result<()> {
    // switch(ws_id) → (true, nil) | (false, err)
    //   Fires on_workspace_switched on success; frontend picks it up via
    //   arbor://workspace-switched and swaps the tab set.
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ws_id: String| -> LuaTuple {
        let h = match handle {
            Some(ref h) => h.clone(),
            None => return boolerr2(lua_ctx, false, Some("app handle unavailable".into())),
        };
        let state = h.state::<crate::AppState>();
        let ws_payload = {
            let mut store = match state.workspaces.lock() {
                Ok(s)  => s,
                Err(e) => return boolerr2(lua_ctx, false, Some(format!("workspaces lock: {e}"))),
            };
            if store.get(&ws_id).is_none() {
                return boolerr2(lua_ctx, false, Some(format!("workspace '{ws_id}' not found")));
            }
            let from = store.active_workspace_id.clone();
            store.active_workspace_id = Some(ws_id.clone());
            let _ = crate::workspace::store::save(&store);
            let ws = store.get(&ws_id).cloned();
            (from, ws)
        };
        let (from_id, Some(ws)) = ws_payload else {
            return boolerr2(lua_ctx, false, Some(format!("workspace '{ws_id}' vanished mid-switch")));
        };
        let mut payload = serde_json::json!({
            "id":          ws.id,
            "name":        ws.name,
            "color_idx":   ws.color_idx,
            "repo_ids":    ws.repo_ids,
            "group_id":    ws.group_id,
            "repo_count":  ws.repo_ids.len(),
        });
        if let Some(f) = from_id {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("from_id".into(), serde_json::Value::String(f));
            }
        }
        let _ = h.emit("arbor://workspace-switched", &payload);
        let h2 = h.clone();
        let payload_str = payload.to_string();
        std::thread::spawn(move || {
            let state = h2.state::<crate::AppState>();
            if let Ok(host) = state.plugin_host.lock() {
                let _ = host.fire_hook("on_workspace_switched", &payload_str);
            };
        });
        boolerr2(lua_ctx, true, None)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ws_table.set("switch", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
