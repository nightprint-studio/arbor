//! Tuple-return helpers.
//!
//! Phase 1+2 convention (see project_plugin_api_refactor memory):
//!   · Programming errors (perm denied, malformed config, wrong arg type) → raise
//!     Lua errors with `mlua::Error::RuntimeError` so plugin authors notice during
//!     development.
//!   · Recoverable runtime failures (I/O, parse, network, git, registry) → return
//!     `(value, nil)` on success, `(nil, err_string)` on failure. The plugin can
//!     ignore the second return value when it doesn't care, or branch on it.

use mlua::Lua;

pub(crate) type LuaTuple = mlua::Result<(mlua::Value, mlua::Value)>;

/// Build the `(val, nil)` success tuple. `val` may itself be nil (e.g. lookup miss).
pub(crate) fn ok2<T: mlua::IntoLua>(lua: &Lua, v: T) -> LuaTuple {
    Ok((v.into_lua(lua)?, mlua::Value::Nil))
}

/// Build the `(nil, err_msg)` failure tuple.
pub(crate) fn err2(lua: &Lua, msg: impl Into<String>) -> LuaTuple {
    let s: String = msg.into();
    Ok((mlua::Value::Nil, mlua::Value::String(lua.create_string(s.as_bytes())?)))
}

/// Convenience for fallible boolean ops: returns `(true, nil)` / `(false, msg)`.
/// Used by APIs that previously returned a silent bool (e.g. `repo.fetch_active_tab`).
pub(crate) fn boolerr2(lua: &Lua, ok_flag: bool, err_when_false: Option<String>) -> LuaTuple {
    if ok_flag {
        Ok((mlua::Value::Boolean(true), mlua::Value::Nil))
    } else {
        let msg = err_when_false.unwrap_or_default();
        Ok((mlua::Value::Boolean(false),
            mlua::Value::String(lua.create_string(msg.as_bytes())?)))
    }
}
