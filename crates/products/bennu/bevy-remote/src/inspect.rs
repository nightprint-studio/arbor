//! Display hints a game attaches to its own types, sent to Arbor so the inspector can draw a colour picker, a
//! slider, a date or an angle where the value alone (a bare number, a list of floats) would say nothing.
//!
//! The hints are written with `#[derive(RemoteInspect)]` and registered with
//! [`RemoteInspectExt::register_inspect`]; `arbor.inspect_hints` answers with `type path → field path → hint`.

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde_json::{Value, json};

/// How one field should be drawn. Plain data: the derive builds it, [`hint_json`] sends it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hint {
    pub color: bool,
    pub date: bool,
    pub angle: bool,
    pub readonly: bool,
    pub range: Option<(f64, f64)>,
    pub step: Option<f64>,
    pub label: Option<String>,
    pub unit: Option<String>,
    /// A sentence shown under the label and on hover: `#[inspect(description = "…")]`, or the `///` docs.
    pub description: Option<String>,
}

/// Implemented by `#[derive(RemoteInspect)]`: the hinted fields of a struct, by field name (or index).
pub trait RemoteInspect {
    fn inspect_hints() -> Vec<(&'static str, Hint)>;
}

/// The hints of every registered type, by the type path BRP knows it under.
#[derive(Resource, Default)]
pub struct InspectRegistry {
    types: BTreeMap<String, Vec<(&'static str, Hint)>>,
}

impl InspectRegistry {
    pub fn insert(&mut self, type_path: impl Into<String>, hints: Vec<(&'static str, Hint)>) {
        self.types.insert(type_path.into(), hints);
    }

    /// `{ type path: { field: hint } }` — exactly what the host reads. The empty field name is the type itself
    /// (its description).
    pub fn to_json(&self) -> Value {
        let mut out = serde_json::Map::new();
        for (type_path, fields) in &self.types {
            let mut by_field = serde_json::Map::new();
            for (field, hint) in fields {
                by_field.insert((*field).to_string(), hint_json(hint));
            }
            out.insert(type_path.clone(), Value::Object(by_field));
        }
        Value::Object(out)
    }
}

/// A hint as JSON, carrying only what was set (the host treats a missing key as "not asked for").
pub fn hint_json(h: &Hint) -> Value {
    let mut out = serde_json::Map::new();
    for (key, on) in [("color", h.color), ("date", h.date), ("angle", h.angle), ("readonly", h.readonly)] {
        if on {
            out.insert(key.into(), Value::Bool(true));
        }
    }
    if let Some((lo, hi)) = h.range {
        out.insert("range".into(), json!([lo, hi]));
    }
    if let Some(step) = h.step {
        out.insert("step".into(), json!(step));
    }
    if let Some(label) = &h.label {
        out.insert("label".into(), json!(label));
    }
    if let Some(unit) = &h.unit {
        out.insert("unit".into(), json!(unit));
    }
    if let Some(description) = &h.description {
        out.insert("description".into(), json!(description));
    }
    Value::Object(out)
}

/// `app.register_inspect::<MyComponent>()`.
pub trait RemoteInspectExt {
    fn register_inspect<T: RemoteInspect + TypePath>(&mut self) -> &mut Self;
}

impl RemoteInspectExt for App {
    fn register_inspect<T: RemoteInspect + TypePath>(&mut self) -> &mut Self {
        self.world_mut().get_resource_or_insert_with(InspectRegistry::default).insert(T::type_path(), T::inspect_hints());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hint_sends_only_what_it_asked_for() {
        assert_eq!(hint_json(&Hint::default()), json!({}));
        let h = Hint { color: true, range: Some((0.0, 1.0)), step: Some(0.01), label: Some("tint".into()), unit: Some("m/s".into()), description: Some("Light colour".into()), ..Default::default() };
        assert_eq!(hint_json(&h), json!({ "color": true, "range": [0.0, 1.0], "step": 0.01, "label": "tint", "unit": "m/s", "description": "Light colour" }));
    }

    #[test]
    fn the_registry_answers_type_then_field() {
        let mut registry = InspectRegistry::default();
        registry.insert("game::Light", vec![("intensity", Hint { range: Some((0.0, 10.0)), ..Default::default() }), ("beam", Hint { angle: true, ..Default::default() })]);
        let json = registry.to_json();
        assert_eq!(json["game::Light"]["intensity"]["range"], json!([0.0, 10.0]));
        assert_eq!(json["game::Light"]["beam"]["angle"], json!(true));
    }

    struct Probe;
    impl RemoteInspect for Probe {
        fn inspect_hints() -> Vec<(&'static str, Hint)> {
            vec![("x", Hint { readonly: true, ..Default::default() })]
        }
    }

    #[test]
    fn a_type_registers_its_own_hints() {
        #[derive(Reflect)]
        struct Wrapper;
        impl RemoteInspect for Wrapper {
            fn inspect_hints() -> Vec<(&'static str, Hint)> {
                Probe::inspect_hints()
            }
        }
        let mut app = App::new();
        app.register_inspect::<Wrapper>();
        let json = app.world().resource::<InspectRegistry>().to_json();
        let (_, fields) = json.as_object().unwrap().iter().next().unwrap();
        assert_eq!(fields["x"]["readonly"], json!(true));
    }
}
