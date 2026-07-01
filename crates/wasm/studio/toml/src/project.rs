//! TOML → `serde_json::Value` projection + the parse helpers.
//!
//! The `DocumentMut` is the source-of-truth for edits + formatting; the
//! projected `Value` is the source-of-truth for navigation + queries (same
//! trick every Studio backend uses). Datetimes project to their textual
//! form; the projection can't carry datetime / array-of-tables back, so the
//! tree pane (which has the live `Item`) keeps the precise kind via
//! [`crate::kind`].

use serde_json::Value;
use toml_edit::{DocumentMut, Item, Value as TomlValue};

/// Parse `text` as a `toml_edit::DocumentMut` AND project to a
/// `serde_json::Value` mirror. Returns the parse error string on failure.
pub fn parse_pair(text: &str) -> (Option<DocumentMut>, Option<Value>, Option<String>) {
    match text.parse::<DocumentMut>() {
        Ok(doc) => {
            let value = doc_to_value(&doc);
            (Some(doc), Some(value), None)
        }
        Err(e) => (None, None, Some(format!("TOML parse error: {e}"))),
    }
}

/// Parse `text` and project to `serde_json::Value`, dropping the AST.
/// `None` on parse error (best-effort, for the cross-ref scanner).
pub fn parse_to_value(text: &str) -> Option<Value> {
    text.parse::<DocumentMut>().ok().map(|d| doc_to_value(&d))
}

pub fn doc_to_value(doc: &DocumentMut) -> Value {
    item_to_value(doc.as_item())
}

pub fn item_to_value(item: &Item) -> Value {
    match item {
        Item::None     => Value::Null,
        Item::Value(v) => toml_value_to_json(v),
        Item::Table(t) => {
            let mut map = serde_json::Map::new();
            for (k, v) in t.iter() {
                map.insert(k.to_string(), item_to_value(v));
            }
            Value::Object(map)
        }
        Item::ArrayOfTables(arr) => {
            let mut items = Vec::with_capacity(arr.len());
            for t in arr.iter() {
                let mut map = serde_json::Map::new();
                for (k, v) in t.iter() {
                    map.insert(k.to_string(), item_to_value(v));
                }
                items.push(Value::Object(map));
            }
            Value::Array(items)
        }
    }
}

pub fn toml_value_to_json(v: &TomlValue) -> Value {
    match v {
        TomlValue::String(s)  => Value::String(s.value().clone()),
        TomlValue::Integer(i) => Value::Number((*i.value()).into()),
        TomlValue::Float(f) => serde_json::Number::from_f64(*f.value())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        TomlValue::Boolean(b) => Value::Bool(*b.value()),
        // Datetimes serialise to their textual form (query / get_value treat
        // the datetime as a string for projection purposes).
        TomlValue::Datetime(d) => Value::String(d.value().to_string()),
        TomlValue::Array(a) => Value::Array(a.iter().map(toml_value_to_json).collect()),
        TomlValue::InlineTable(t) => {
            let mut map = serde_json::Map::new();
            for (k, v) in t.iter() {
                map.insert(k.to_string(), toml_value_to_json(v));
            }
            Value::Object(map)
        }
    }
}

/// Sniff the document's indent string for the FE indent pill — first
/// indented line wins, else two spaces.
pub fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        let leading: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !leading.is_empty() {
            return leading;
        }
    }
    "  ".into()
}
