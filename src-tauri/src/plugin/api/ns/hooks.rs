//! `arbor.hooks` — introspection of the built-in hook catalog.
//!
//! Plugins can discover what hooks the host fires and what fields each
//! payload carries — without consulting external docs.
//!
//!   arbor.hooks.list() → { hook_def, hook_def, … }
//!   arbor.hooks.describe(name) → hook_def | nil
//!
//! hook_def shape:
//!   { name        = string,                 -- e.g. "on_repo_open"
//!     category    = string,                 -- e.g. "repo"
//!     description = string,
//!     ctx         = { field, field, … } }   -- ordered list (sequence)
//!
//! field shape:
//!   { name        = string,
//!     type        = string,                 -- "string"|"number"|"boolean"|"string[]"|"object"
//!     required    = boolean,
//!     description = string }
//!
//! Action hooks fired via `arbor.events.emit`, `arbor.command.register`,
//! or `arbor.job.spawn{on_done = ...}` are not in the catalog — they're
//! plugin-defined. `describe()` returns nil for those.

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::hook_catalog::{HOOK_CATALOG, HookDef, find};

pub(crate) fn install(_ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let hooks_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(lua, &hooks_table)?;
    install_describe(lua, &hooks_table)?;

    arbor.set("hooks", hooks_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(lua: &Lua, t: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, ()| {
        let out = lua_ctx.create_table()?;
        for (i, def) in HOOK_CATALOG.iter().enumerate() {
            out.set(i + 1, build_hook_def(lua_ctx, def)?)?;
        }
        Ok(out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_describe(lua: &Lua, t: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, name: String| {
        match find(&name) {
            Some(def) => Ok(mlua::Value::Table(build_hook_def(lua_ctx, def)?)),
            None      => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("describe", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// Build a single hook_def Lua table from a catalog entry.
fn build_hook_def(lua_ctx: &Lua, def: &HookDef) -> mlua::Result<Table> {
    let t = lua_ctx.create_table()?;
    t.set("name",        def.name)?;
    t.set("category",    def.category)?;
    t.set("description", def.description)?;
    let ctx = lua_ctx.create_table()?;
    for (i, f) in def.ctx.iter().enumerate() {
        let ft = lua_ctx.create_table()?;
        ft.set("name",        f.name)?;
        ft.set("type",        f.ty.as_str())?;
        ft.set("required",    f.required)?;
        ft.set("description", f.description)?;
        ctx.set(i + 1, ft)?;
    }
    t.set("ctx", ctx)?;
    Ok(t)
}
