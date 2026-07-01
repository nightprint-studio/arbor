//! Node-kind + preview strings over the projected `serde_json::Value`.
//!
//! `SimpleFormat`/`DefaultBackend` operate on the JSON projection, never the
//! live `toml_edit::Item`, so these mirror the pre-extraction `kind_for_value`
//! / `preview_for_value` (the JSON-mirror variants). That means TOML loses
//! datetime / array-of-tables precision for query hits + tree rows projected
//! from JSON — exactly the documented pre-extraction behavior (the previous
//! `DefaultBackend`-free code had the same `kind_for_value` fallback for
//! query hits; navigation now uniformly goes through the projection).
//!
//! FROZEN F11 — the kind STRINGS stay distinct (`table` / `inline_table` /
//! `array` / `array_of_tables` / `string` / `integer` / `float` / `bool` /
//! `datetime`); the descriptor's `kind_palette` keys match these.

use serde_json::Value;

const PREVIEW_MAX_CHARS: usize = 64;

/// Kind string for a projected value node. Containers map to
/// `inline_table` (object) / `array`; the projection can't distinguish
/// block-table vs inline-table vs array-of-tables, so this is the
/// closest analogue (same loss the pre-extraction query path had).
pub fn node_kind(v: &Value) -> String {
    match v {
        Value::Null      => "string",
        Value::Bool(_)   => "bool",
        Value::Number(n) => {
            if n.is_f64() && !n.is_i64() && !n.is_u64() {
                "float"
            } else {
                "integer"
            }
        }
        Value::String(_) => "string",
        Value::Array(_)  => "array",
        Value::Object(_) => "inline_table",
    }
    .to_string()
}

/// Short preview string for a projected value node.
pub fn preview_for(v: &Value) -> String {
    match v {
        Value::Object(m) => format!("{{{} keys}}", m.len()),
        Value::Array(a)  => format!("[{} items]", a.len()),
        Value::String(s) => {
            let mut out = String::with_capacity(s.len().min(PREVIEW_MAX_CHARS) + 2);
            out.push('"');
            for (i, ch) in s.chars().enumerate() {
                if i >= PREVIEW_MAX_CHARS {
                    out.push('…');
                    break;
                }
                out.push(ch);
            }
            out.push('"');
            out
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b)   => b.to_string(),
        Value::Null      => "null".to_string(),
    }
}
