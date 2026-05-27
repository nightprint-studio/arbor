//! `PluginValue` — the in-process value type bridged across runtimes.
//!
//! Why a dedicated enum (decision **D1** in the architecture doc):
//!
//! - In-process bridging is roughly 5–10× cheaper than going through
//!   `serde_json::Value` everywhere (no string-keyed `Number` boxing, no
//!   re-parsing).
//! - A generic-over-`T` API would explode monomorphisation across runtimes
//!   (mlua → wasm → …) and across every contributed function.
//!
//! The enum stays small (8 variants) and convertible to/from `serde_json::Value`
//! so that domain crates can keep using `serde` types internally and only
//! bridge at the API boundary via [`PluginValue::from_serializable`].

use std::collections::BTreeMap;

use crate::error::PluginError;

/// The bridging value passed between the host (Rust) and a plugin runtime.
///
/// `Map` keys are always `String` for cross-runtime parity (Lua tables with
/// non-string keys do not survive the trip into wasm or JSON anyway).
#[derive(Debug, Clone, PartialEq)]
pub enum PluginValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<PluginValue>),
    Map(BTreeMap<String, PluginValue>),
}

impl PluginValue {
    /// Build a `PluginValue` from any `serde::Serialize` value.
    ///
    /// Goes through `serde_json::Value` internally — fine for the typical
    /// HTTP / GraphQL response shapes that already serialise that way, and it
    /// keeps the conversion logic to one place.
    pub fn from_serializable<T: serde::Serialize>(v: &T) -> Result<Self, PluginError> {
        let json = serde_json::to_value(v).map_err(|e| PluginError::other(e.to_string()))?;
        Ok(Self::from_json(json))
    }

    /// Convert a `serde_json::Value` into a `PluginValue`.
    ///
    /// JSON numbers that fit into `i64` are mapped to `Int`; everything else
    /// (including fractional and very large numbers) becomes `Float`.
    pub fn from_json(json: serde_json::Value) -> Self {
        match json {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Int(i)
                } else if let Some(u) = n.as_u64() {
                    // `u64` values past `i64::MAX` lose range — accept the
                    // float fallback rather than silently truncating.
                    if u <= i64::MAX as u64 {
                        Self::Int(u as i64)
                    } else {
                        Self::Float(u as f64)
                    }
                } else if let Some(f) = n.as_f64() {
                    Self::Float(f)
                } else {
                    Self::Null
                }
            }
            serde_json::Value::String(s) => Self::String(s),
            serde_json::Value::Array(a) => Self::List(a.into_iter().map(Self::from_json).collect()),
            serde_json::Value::Object(o) => Self::Map(
                o.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }

    /// Convert a `PluginValue` back into a `serde_json::Value`.
    ///
    /// `Bytes` serialise as a JSON array of integers (lossy on the other
    /// direction — JSON has no native byte-string). Non-finite floats become
    /// `Null` since JSON cannot represent NaN / Infinity.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(b) => serde_json::Value::Bool(*b),
            Self::Int(i) => serde_json::Value::Number((*i).into()),
            Self::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Self::String(s) => serde_json::Value::String(s.clone()),
            Self::Bytes(b) => {
                serde_json::Value::Array(b.iter().map(|x| (*x as i64).into()).collect())
            }
            Self::List(l) => serde_json::Value::Array(l.iter().map(Self::to_json).collect()),
            Self::Map(m) => serde_json::Value::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
        }
    }

    // ── Typed accessors ────────────────────────────────────────────────────

    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(b) = self { Some(*b) } else { None }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            // Be generous: floats with no fractional part round-trip cleanly.
            Self::Float(f) if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 => {
                Some(*f as i64)
            }
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        if let Self::String(s) = self { Some(s.as_str()) } else { None }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        if let Self::Bytes(b) = self { Some(b.as_slice()) } else { None }
    }

    pub fn as_list(&self) -> Option<&[PluginValue]> {
        if let Self::List(l) = self { Some(l.as_slice()) } else { None }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, PluginValue>> {
        if let Self::Map(m) = self { Some(m) } else { None }
    }
}

impl Default for PluginValue {
    fn default() -> Self {
        Self::Null
    }
}

// ── Ergonomic key access on `Map` ──────────────────────────────────────────

/// Convenience methods on the `Map` payload of a [`PluginValue`].
///
/// Implemented in terms of `BTreeMap<String, PluginValue>` so it works equally
/// well when you've already destructured (`if let Map(m) = …`) and when you
/// call through [`PluginValue::as_map`].
pub trait PluginMapExt {
    fn get_string(&self, key: &str) -> Result<String, PluginError>;
    fn get_int(&self, key: &str) -> Result<i64, PluginError>;
    fn get_bool(&self, key: &str) -> Result<bool, PluginError>;
    fn get_list(&self, key: &str) -> Result<&[PluginValue], PluginError>;
    fn get_map(&self, key: &str) -> Result<&BTreeMap<String, PluginValue>, PluginError>;

    fn get_string_opt(&self, key: &str) -> Result<Option<String>, PluginError>;
    fn get_int_opt(&self, key: &str) -> Result<Option<i64>, PluginError>;
    fn get_bool_opt(&self, key: &str) -> Result<Option<bool>, PluginError>;
}

fn missing(key: &str) -> PluginError {
    PluginError::bad_args(format!("missing key '{key}'"))
}

fn wrong_type(key: &str, expected: &str) -> PluginError {
    PluginError::bad_args(format!("'{key}' must be a {expected}"))
}

impl PluginMapExt for BTreeMap<String, PluginValue> {
    fn get_string(&self, key: &str) -> Result<String, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Err(missing(key)),
            Some(PluginValue::String(s)) => Ok(s.clone()),
            Some(_) => Err(wrong_type(key, "string")),
        }
    }

    fn get_int(&self, key: &str) -> Result<i64, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Err(missing(key)),
            Some(v) => v.as_int().ok_or_else(|| wrong_type(key, "integer")),
        }
    }

    fn get_bool(&self, key: &str) -> Result<bool, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Err(missing(key)),
            Some(PluginValue::Bool(b)) => Ok(*b),
            Some(_) => Err(wrong_type(key, "boolean")),
        }
    }

    fn get_list(&self, key: &str) -> Result<&[PluginValue], PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Err(missing(key)),
            Some(PluginValue::List(l)) => Ok(l.as_slice()),
            Some(_) => Err(wrong_type(key, "list")),
        }
    }

    fn get_map(&self, key: &str) -> Result<&BTreeMap<String, PluginValue>, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Err(missing(key)),
            Some(PluginValue::Map(m)) => Ok(m),
            Some(_) => Err(wrong_type(key, "table")),
        }
    }

    fn get_string_opt(&self, key: &str) -> Result<Option<String>, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Ok(None),
            Some(PluginValue::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(wrong_type(key, "string")),
        }
    }

    fn get_int_opt(&self, key: &str) -> Result<Option<i64>, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Ok(None),
            Some(v) => v
                .as_int()
                .map(Some)
                .ok_or_else(|| wrong_type(key, "integer")),
        }
    }

    fn get_bool_opt(&self, key: &str) -> Result<Option<bool>, PluginError> {
        match self.get(key) {
            None | Some(PluginValue::Null) => Ok(None),
            Some(PluginValue::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(wrong_type(key, "boolean")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_round_trip_preserves_primitives() {
        let v = json!({
            "name":  "arbor",
            "count": 42,
            "ratio": 1.5,
            "flag":  true,
            "tags":  ["a", "b", "c"],
            "inner": { "k": 1 },
            "void":  null,
        });
        let pv = PluginValue::from_json(v.clone());
        assert_eq!(pv.to_json(), v);
    }

    #[test]
    fn integers_stay_integers() {
        let pv = PluginValue::from_json(json!(7));
        assert!(matches!(pv, PluginValue::Int(7)));
    }

    #[test]
    fn fractional_numbers_become_float() {
        let pv = PluginValue::from_json(json!(1.5));
        assert!(matches!(pv, PluginValue::Float(_)));
    }

    #[test]
    fn from_serializable_handles_structs() {
        #[derive(serde::Serialize)]
        struct S {
            number: u32,
            title: String,
        }
        let pv = PluginValue::from_serializable(&S {
            number: 17,
            title: "MR".into(),
        })
        .unwrap();
        let m = pv.as_map().expect("map");
        assert_eq!(m.get_int("number").unwrap(), 17);
        assert_eq!(m.get_string("title").unwrap(), "MR");
    }

    #[test]
    fn map_helpers_reject_wrong_type() {
        let mut m: BTreeMap<String, PluginValue> = BTreeMap::new();
        m.insert("title".into(), PluginValue::Int(42));
        let err = m.get_string("title").unwrap_err();
        assert!(matches!(err, PluginError::BadArgs(_)));
    }

    #[test]
    fn map_helpers_optional_missing_is_none() {
        let m: BTreeMap<String, PluginValue> = BTreeMap::new();
        assert_eq!(m.get_string_opt("missing").unwrap(), None);
    }

    #[test]
    fn non_finite_floats_become_null_in_json() {
        let pv = PluginValue::Float(f64::INFINITY);
        assert_eq!(pv.to_json(), serde_json::Value::Null);
    }
}
