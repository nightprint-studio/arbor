//! Lua ↔ JSON / duration conversion helpers.

use mlua::{Lua, LuaSerdeExt, SerializeOptions};

use arbor_plugin_types::prelude::parse_duration_secs;

/// How every JSON document the host hands a plugin is lowered into Lua.
///
/// The load-bearing part is `serialize_none_to_null(false)`: mlua's default turns JSON `null`
/// into its own null sentinel — a light userdata that is **truthy** and is not `nil`. A plugin
/// reading a nullable field then sees a value where the API documents its absence, and the
/// symptom is never local: `if entry.etag then`, `x or 0`, `until cursor == nil` all take the
/// wrong branch, in Lua, far from the conversion that produced it.
///
/// Lua has one absence and it is `nil`; a second one that only the host can produce is a
/// distinction no plugin author asked for. The array metatable stays on, so an empty list
/// still round-trips back out as `[]` rather than `{}`.
pub const LUA_JSON: SerializeOptions = SerializeOptions::new()
    .serialize_none_to_null(false)
    .serialize_unit_to_null(false);

pub fn lua_value_to_json(val: mlua::Value) -> Option<serde_json::Value> {
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

/// The one way anything serialisable becomes a Lua value. See [`LUA_JSON`] for why it is not
/// `lua.to_value`.
///
/// Every namespace that answers a plugin goes through here — or through [`json_to_lua`],
/// which is this with the document already parsed. A `lua.to_value` call anywhere in the
/// plugin surface is a namespace whose nullable fields behave differently from the others.
pub fn serde_to_lua<T>(lua: &Lua, val: &T) -> mlua::Result<mlua::Value>
where
    T: serde::Serialize + ?Sized,
{
    lua.to_value_with(val, LUA_JSON)
}

/// [`serde_to_lua`] for a JSON document. Typed on `serde_json::Value` because most of the
/// host's answers already are one, and the signature says so at the call site.
pub fn json_to_lua(lua: &Lua, val: &serde_json::Value) -> mlua::Result<mlua::Value> {
    serde_to_lua(lua, val)
}

/// Same, from a JSON document that has not been parsed yet.
pub fn json_str_to_lua(lua: &Lua, s: &str) -> mlua::Result<mlua::Value> {
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
    json_to_lua(lua, &v)
}

/// Coerce a Lua value into a duration in seconds. Numbers are seconds;
/// strings go through `parse_duration_secs` (`"30s"`, `"5m"`, `"PT1H30M"`, …).
pub fn lua_value_to_duration_secs(v: &mlua::Value) -> std::result::Result<u64, String> {
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
pub fn strip_utf8_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant the whole plugin surface rests on. mlua's default would put its own null
    /// sentinel here — a light userdata that is truthy — and every plugin reading a nullable
    /// field would take the wrong branch.
    #[test]
    fn json_null_reaches_lua_as_nil() {
        let lua = Lua::new();
        let table = match json_to_lua(&lua, &serde_json::json!({ "etag": null, "size": 7 }))
            .expect("conversion")
        {
            mlua::Value::Table(t) => t,
            other => panic!("expected a table, got {}", other.type_name()),
        };

        let etag: mlua::Value = table.get("etag").expect("etag");
        assert!(etag.is_nil(), "a null field must be nil, got {}", etag.type_name());
        // And the absence is indistinguishable from a key that was never there, which is what
        // makes `if x.etag then` read correctly.
        let missing: mlua::Value = table.get("nothing").expect("missing key");
        assert!(missing.is_nil());
        assert_eq!(table.get::<u64>("size").expect("size"), 7);
    }

    /// A `none` at the top of an answer: the same rule, and the one `list_all`-style loops
    /// exit on.
    #[test]
    fn top_level_null_is_nil() {
        let lua = Lua::new();
        let v = json_to_lua(&lua, &serde_json::Value::Null).expect("conversion");
        assert!(v.is_nil());
    }
}
