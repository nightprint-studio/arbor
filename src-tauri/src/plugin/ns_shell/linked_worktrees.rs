//! `arbor.linked_worktrees` — read + sync-toggle access for plugins.
//!
//! The live registry now lives **out-of-process** in `corvus-be`; this namespace
//! reads / writes the shared `linked_worktrees.toml` (the same file corvus-be
//! persists, via `crate::linked_worktrees::{load, save}`), so plugins see and
//! toggle the same data without a cross-process call. Plugins can introspect
//! membership and toggle sync, but not create or delete links.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::ns_shell::ctx_ext::ApiCtxExt;
use arbor_plugin_core::prelude::{boolerr2, ApiCtx, LuaTuple};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let lw_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(lua, &lw_table)?;
    install_get(lua, &lw_table)?;
    install_set_sync_enabled(ctx, lua, &lw_table)?;

    arbor.set("linked_worktrees", lw_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(lua: &Lua, lw_table: &Table) -> Result<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ()| {
            let reg = crate::linked_worktrees::load();
            let arr = lua_ctx.create_table()?;
            for (i, l) in reg.list().iter().enumerate() {
                let t = lua_ctx.create_table()?;
                t.set("id", l.id.clone())?;
                t.set("name", l.name.clone())?;
                t.set("sync_enabled", l.sync_enabled)?;
                t.set("member_count", l.members.len() as i64)?;
                arr.set(i + 1, t)?;
            }
            Ok(mlua::Value::Table(arr))
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    lw_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(lua: &Lua, lw_table: &Table) -> Result<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, id: String| {
            let reg = crate::linked_worktrees::load();
            match reg.get(&id) {
                Some(l) => Ok(lua_ctx.to_value(l)?),
                None => Ok(mlua::Value::Nil),
            }
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    lw_table.set("get", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_sync_enabled(ctx: &ApiCtx, lua: &Lua, lw_table: &Table) -> Result<()> {
    let handle = ctx.app_handle();
    let fn_ = lua
        .create_function(move |lua_ctx, (id, enabled): (String, bool)| -> LuaTuple {
            let mut reg = crate::linked_worktrees::load();
            if let Err(e) = reg.set_sync_enabled(&id, enabled) {
                return boolerr2(lua_ctx, false, Some(format!("set_sync_enabled: {e}")));
            }
            let _ = crate::linked_worktrees::save(&reg);
            if let Some(ref h) = handle {
                let _ = h.emit("arbor://worktree-links-changed", serde_json::json!({}));
            }
            boolerr2(lua_ctx, true, None)
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    lw_table.set("set_sync_enabled", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
