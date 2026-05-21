//! Lua ↔ JSON / duration conversion helpers.

use mlua::{Lua, LuaSerdeExt};

use crate::plugin::runtime::parse_duration_secs;

pub(crate) fn lua_value_to_json(val: mlua::Value) -> Option<serde_json::Value> {
    match val {
        mlua::Value::Nil             => None,
        mlua::Value::Boolean(b)      => Some(serde_json::Value::Bool(b)),
        mlua::Value::Integer(i)      => Some(serde_json::Value::Number(i.into())),
        mlua::Value::Number(f)       => serde_json::Number::from_f64(f).map(serde_json::Value::Number),
        mlua::Value::String(s)       => Some(serde_json::Value::String(
            s.to_str().map(|it| it.to_string()).unwrap_or("".to_string())
        )),
        mlua::Value::Table(_) => {
            // Tables are serialized via LuaSerdeExt — handled separately
            None
        }
        _ => None,
    }
}

pub(crate) fn json_to_lua(lua: &Lua, val: &serde_json::Value) -> mlua::Result<mlua::Value> {
    lua.to_value(val)
}

/// Coerce a Lua value into a duration in seconds. Numbers are seconds;
/// strings go through `parse_duration_secs` (`"30s"`, `"5m"`, `"PT1H30M"`, …).
pub(crate) fn lua_value_to_duration_secs(v: &mlua::Value) -> std::result::Result<u64, String> {
    match v {
        mlua::Value::Integer(i) if *i >= 0 => Ok(*i as u64),
        mlua::Value::Number(n)  if *n >= 0.0 && n.is_finite() => Ok(*n as u64),
        mlua::Value::String(s) => {
            let owned = s.to_str().map_err(|e| e.to_string())?.to_string();
            parse_duration_secs(&owned)
        }
        other => Err(format!(
            "expected duration as number (seconds) or string ('30s'/'5m'/'2h'/'1d'/'PT…'), got {}",
            other.type_name()
        )),
    }
}

/// Strip UTF-8 BOM (`EF BB BF`) from the start of a string read off disk.
/// PowerShell 5.1's `Set-Content -Encoding UTF8` writes a BOM; serde_json and
/// serde_yaml_ng don't accept it and fail with "expected value at line 1 column
/// 1". Apply this before feeding any structured-edit parser.
pub(crate) fn strip_utf8_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}
