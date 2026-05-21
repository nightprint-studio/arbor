//! `arbor.text` — string helpers backed by the `regex` crate (PCRE-ish).
//!
//! Calling convention (Phase 1+2):
//!   · replace / contains take a single config table (so adding flags is
//!     non-breaking).
//!   · A bad regex returns `(nil, err)` rather than raising — callers
//!     usually want to fall through with a default.
//!   · `find_all` keeps positional args (just (content, pattern)).
//!   · `escape` is pure compute, returns a single string.

use mlua::{IntoLua, Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};

pub(crate) fn install(_ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let text_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_replace(lua, &text_table)?;
    install_contains(lua, &text_table)?;
    install_find_all(lua, &text_table)?;
    install_escape(lua, &text_table)?;

    arbor.set("text", text_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_replace(lua: &Lua, text_table: &Table) -> Result<()> {
    // replace{content, pattern, replacement, plain?} → (new_content, count) | (nil, err)
    //
    // Three return values: when plain is false (regex mode), capture
    // groups inside `replacement` may be referenced as `$1`, `$name`, …
    // The third return slot is the err string (nil on success); the
    // common ignore-error pattern is `local out = arbor.text.replace{…}`.
    let fn_ = lua.create_function(|lua_ctx, cfg: mlua::Table| -> mlua::Result<(mlua::Value, mlua::Value, mlua::Value)> {
        let content: String = cfg.get("content").map_err(|_|
            mlua::Error::RuntimeError("arbor.text.replace: 'content' is required".into()))?;
        let pattern: String = cfg.get("pattern").map_err(|_|
            mlua::Error::RuntimeError("arbor.text.replace: 'pattern' is required".into()))?;
        let replacement: String = cfg.get("replacement").map_err(|_|
            mlua::Error::RuntimeError("arbor.text.replace: 'replacement' is required".into()))?;
        let plain: bool = cfg.get::<Option<bool>>("plain").unwrap_or(None).unwrap_or(false);

        let nil = mlua::Value::Nil;
        if plain {
            let count = if pattern.is_empty() { 0usize } else { content.matches(&pattern).count() };
            let out = content.replace(&pattern, &replacement);
            Ok((out.into_lua(lua_ctx)?, (count as i64).into_lua(lua_ctx)?, nil))
        } else {
            match regex::Regex::new(&pattern) {
                Ok(re) => {
                    let count = re.find_iter(&content).count();
                    let out = re.replace_all(&content, replacement.as_str()).into_owned();
                    Ok((out.into_lua(lua_ctx)?, (count as i64).into_lua(lua_ctx)?, nil))
                }
                Err(e) => Ok((
                    nil.clone(),
                    nil,
                    mlua::Value::String(lua_ctx.create_string(format!("text.replace regex: {e}").as_bytes())?),
                )),
            }
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    text_table.set("replace", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_contains(lua: &Lua, text_table: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, cfg: mlua::Table| -> LuaTuple {
        let content: String = cfg.get("content").map_err(|_|
            mlua::Error::RuntimeError("arbor.text.contains: 'content' is required".into()))?;
        let pattern: String = cfg.get("pattern").map_err(|_|
            mlua::Error::RuntimeError("arbor.text.contains: 'pattern' is required".into()))?;
        let plain: bool = cfg.get::<Option<bool>>("plain").unwrap_or(None).unwrap_or(false);

        if plain {
            ok2(lua_ctx, content.contains(&pattern))
        } else {
            match regex::Regex::new(&pattern) {
                Ok(re) => ok2(lua_ctx, re.is_match(&content)),
                Err(e) => err2(lua_ctx, format!("text.contains regex: {e}")),
            }
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    text_table.set("contains", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_find_all(lua: &Lua, text_table: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, (content, pattern): (String, String)| -> LuaTuple {
        let re = match regex::Regex::new(&pattern) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("text.find_all regex: {e}")),
        };
        let arr = lua_ctx.create_table()?;
        for (i, m) in re.find_iter(&content).enumerate() {
            arr.set(i + 1, m.as_str())?;
        }
        ok2(lua_ctx, arr)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    text_table.set("find_all", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_escape(lua: &Lua, text_table: &Table) -> Result<()> {
    let fn_ = lua.create_function(|_lua_ctx, s: String| {
        Ok(regex::escape(&s))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    text_table.set("escape", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
