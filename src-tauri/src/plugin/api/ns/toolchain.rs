//! `arbor.toolchain` — toolchain registry CRUD + env resolution.
//!
//! Calling convention (Phase 1+2):
//!   · Read ops return `(value, nil)` / `(nil, err)`. Permission denied
//!     and bad arg types still raise.
//!   · `env{kind, id?}` takes a config table for forward-compat.
//!   · Mutating ops return `(true, nil)` / `(false, err)`.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::convert::json_to_lua;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let toolchain_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(ctx, lua, &toolchain_table)?;
    install_active(ctx, lua, &toolchain_table)?;
    install_env(ctx, lua, &toolchain_table)?;
    install_detect(ctx, lua, &toolchain_table)?;
    install_add(ctx, lua, &toolchain_table)?;
    install_remove(ctx, lua, &toolchain_table)?;
    install_set_active(ctx, lua, &toolchain_table)?;

    arbor.set("toolchain", toolchain_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, kind: String| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.list: toolchain = \"read\" (or higher) permission required".to_string()
            ));
        }
        let Some(ref handle) = h else {
            return err2(lua_ctx, "toolchain.list: app handle unavailable");
        };
        let state = handle.state::<crate::AppState>();
        let entries = match state.toolchain_registry.lock() {
            Ok(mut g) => g.list(&kind),
            Err(e)    => return err2(lua_ctx, format!("toolchain.list lock: {e}")),
        };
        let json = match serde_json::to_value(&entries) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("toolchain.list encode: {e}")),
        };
        ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_active(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, kind: String| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.active: toolchain = \"read\" (or higher) permission required".to_string()
            ));
        }
        let Some(ref handle) = h else {
            return err2(lua_ctx, "toolchain.active: app handle unavailable");
        };
        let state = handle.state::<crate::AppState>();
        let entry = match state.toolchain_registry.lock() {
            Ok(mut g) => g.active(&kind),
            Err(e)    => return err2(lua_ctx, format!("toolchain.active lock: {e}")),
        };
        match entry {
            None    => ok2(lua_ctx, mlua::Value::Nil),
            Some(e) => {
                let json = match serde_json::to_value(&e) {
                    Ok(v)  => v,
                    Err(er) => return err2(lua_ctx, format!("toolchain.active encode: {er}")),
                };
                ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
            }
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("active", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_env(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.env: toolchain = \"read\" (or higher) permission required".to_string()
            ));
        }
        let kind: String = cfg.get("kind").map_err(|_|
            mlua::Error::RuntimeError("arbor.toolchain.env: 'kind' is required".into()))?;
        let id: Option<String> = cfg.get::<Option<String>>("id").unwrap_or(None);

        let Some(ref handle) = h else {
            return err2(lua_ctx, "toolchain.env: app handle unavailable");
        };
        let state = handle.state::<crate::AppState>();
        let env = match state.toolchain_registry.lock() {
            Ok(mut g) => g.env_for(&kind, id.as_deref()),
            Err(e)    => return err2(lua_ctx, format!("toolchain.env lock: {e}")),
        };
        let json = match serde_json::to_value(&env) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("toolchain.env encode: {e}")),
        };
        ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("env", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_detect(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, kind: String| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.detect: toolchain = \"read\" (or higher) permission required".to_string()
            ));
        }
        let Some(ref handle) = h else {
            return err2(lua_ctx, "toolchain.detect: app handle unavailable");
        };
        let state = handle.state::<crate::AppState>();
        let entries = match state.toolchain_registry.lock() {
            Ok(g) => g.detect(&kind),
            Err(e)    => return err2(lua_ctx, format!("toolchain.detect lock: {e}")),
        };
        let json = match serde_json::to_value(&entries) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("toolchain.detect encode: {e}")),
        };
        ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("detect", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_add(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let write = ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, (kind, entry_table): (String, mlua::Table)| -> LuaTuple {
        if !write {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.add: toolchain = \"write\" permission required".to_string()
            ));
        }
        let Some(ref handle) = h else {
            return boolerr2(lua_ctx, false, Some("toolchain.add: app handle unavailable".into()));
        };
        let entry: crate::plugin::toolchain::ToolchainEntry = lua_ctx
            .from_value(mlua::Value::Table(entry_table))
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.toolchain.add: invalid entry: {e}")))?;
        match handle.state::<crate::AppState>().toolchain_registry.lock() {
            Ok(mut g) => { g.add(&kind, entry); boolerr2(lua_ctx, true, None) }
            Err(e)    => boolerr2(lua_ctx, false, Some(format!("toolchain.add lock: {e}"))),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("add", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_remove(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let write = ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, (kind, id): (String, String)| -> LuaTuple {
        if !write {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.remove: toolchain = \"write\" permission required".to_string()
            ));
        }
        let Some(ref handle) = h else {
            return boolerr2(lua_ctx, false, Some("toolchain.remove: app handle unavailable".into()));
        };
        match handle.state::<crate::AppState>().toolchain_registry.lock() {
            Ok(mut g) => { g.remove(&kind, &id); boolerr2(lua_ctx, true, None) }
            Err(e)    => boolerr2(lua_ctx, false, Some(format!("toolchain.remove lock: {e}"))),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("remove", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_active(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let write = ctx.toolchain_write;
    let h = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, (kind, id): (String, String)| -> LuaTuple {
        if !write {
            return Err(mlua::Error::RuntimeError(
                "arbor.toolchain.set_active: toolchain = \"write\" permission required".to_string()
            ));
        }
        let Some(ref handle) = h else {
            return boolerr2(lua_ctx, false, Some("toolchain.set_active: app handle unavailable".into()));
        };
        match handle.state::<crate::AppState>().toolchain_registry.lock() {
            Ok(mut g) => { g.set_active(&kind, &id); boolerr2(lua_ctx, true, None) }
            Err(e)    => boolerr2(lua_ctx, false, Some(format!("toolchain.set_active lock: {e}"))),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("set_active", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
