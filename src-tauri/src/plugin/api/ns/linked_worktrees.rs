//! `arbor.linked_worktrees` — read-only access for plugins. Links are
//! user-managed; plugins can introspect membership and toggle sync, but
//! not create or delete links.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let lw_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(ctx, lua, &lw_table)?;
    install_get(ctx, lua, &lw_table)?;
    install_set_sync_enabled(ctx, lua, &lw_table)?;

    arbor.set("linked_worktrees", lw_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, lw_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ()| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let reg = state.linked_worktrees.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let arr = lua_ctx.create_table()?;
        for (i, l) in reg.list().iter().enumerate() {
            let t = lua_ctx.create_table()?;
            t.set("id",            l.id.clone())?;
            t.set("name",          l.name.clone())?;
            t.set("sync_enabled",  l.sync_enabled)?;
            t.set("member_count",  l.members.len() as i64)?;
            arr.set(i + 1, t)?;
        }
        Ok(mlua::Value::Table(arr))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    lw_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(ctx: &ApiCtx, lua: &Lua, lw_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, id: String| {
        let h = match handle { Some(ref h) => h.clone(), None => return Ok(mlua::Value::Nil) };
        let state = h.state::<crate::AppState>();
        let reg = state.linked_worktrees.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match reg.get(&id) {
            Some(l) => Ok(lua_ctx.to_value(l)?),
            None    => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    lw_table.set("get", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_sync_enabled(ctx: &ApiCtx, lua: &Lua, lw_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, (id, enabled): (String, bool)| -> LuaTuple {
        let h = match handle {
            Some(ref h) => h.clone(),
            None => return boolerr2(lua_ctx, false, Some("app handle unavailable".into())),
        };
        let state = h.state::<crate::AppState>();
        let mut reg = state.linked_worktrees.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if let Err(e) = reg.set_sync_enabled(&id, enabled) {
            return boolerr2(lua_ctx, false, Some(format!("set_sync_enabled: {e}")));
        }
        let _ = crate::linked_worktrees::save(&reg);
        let _ = h.emit("arbor://worktree-links-changed", serde_json::json!({}));
        boolerr2(lua_ctx, true, None)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    lw_table.set("set_sync_enabled", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
