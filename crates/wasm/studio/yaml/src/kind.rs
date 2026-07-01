//! Node-kind + preview strings over the projected `serde_json::Value`.
//!
//! `SimpleFormat`/`DefaultBackend` operate on the JSON projection. YAML
//! has first-class `null`, so (unlike the JSON set) `null` stays a distinct
//! kind — FROZEN F11: the FE chip palette keys on it.

use serde_json::Value;

const PREVIEW_MAX_CHARS: usize = 64;

/// Kind string for a projected value node. YAML keeps `null` distinct.
pub fn node_kind(v: &Value) -> String {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "float"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
    .to_string()
}

/// Short preview string for a projected value node.
pub fn preview_for(v: &Value) -> String {
    let s = match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Object(m) => format!("{{{} fields}}", m.len()),
    };
    truncate_preview(&s)
}

fn truncate_preview(s: &str) -> String {
    if s.chars().count() <= PREVIEW_MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(PREVIEW_MAX_CHARS).collect();
    out.push('…');
    out
}
