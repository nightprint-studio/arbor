//! `arbor.json.encode` / `arbor.json.decode`.

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(_ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let json_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    let encode_fn = lua
        .create_function(|lua_ctx, (val, opts): (mlua::Value, Option<mlua::Table>)| {
            let json: serde_json::Value = lua_ctx
                .from_value(val)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            // Optional `{ pretty = true }` opts the call into 2-space indent
            // output. Plugins dumping JSON into a UI code block want this;
            // serialization back over the wire wants the compact default.
            let pretty = opts
                .as_ref()
                .and_then(|t| t.get::<bool>("pretty").ok())
                .unwrap_or(false);
            let s = if pretty {
                serde_json::to_string_pretty(&json)
            } else {
                serde_json::to_string(&json)
            }.map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            Ok((mlua::Value::String(lua_ctx.create_string(s.as_bytes())?), mlua::Value::Nil))
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    json_table.set("encode", encode_fn).map_err(|e| AppError::Plugin(e.to_string()))?;

    let decode_fn = lua
        .create_function(|lua_ctx, s: String| {
            match serde_json::from_str::<serde_json::Value>(&s) {
                Ok(v) => {
                    let lv = lua_ctx.to_value(&v)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                    Ok((lv, mlua::Value::Nil))
                }
                Err(e) => Ok((mlua::Value::Nil, mlua::Value::String(
                    lua_ctx.create_string(e.to_string().as_bytes())?
                ))),
            }
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    json_table.set("decode", decode_fn).map_err(|e| AppError::Plugin(e.to_string()))?;

    arbor.set("json", json_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
