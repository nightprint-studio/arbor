//! The game's type registry, as `registry.schema` reports it — and what an editor can make of it.
//!
//! BRP hands back one big map `type path → schema` for every type the game registered. Two things
//! are built from it here:
//!
//! * a **listing** of the registered components and resources (searchable, paged), so a UI can
//!   offer "insert component" without the user typing a Rust path from memory;
//! * a **[`Shape`]** per type: the small, recursive description a typed editor needs — "this is a
//!   struct with an `f32` and an enum", "this is a list of strings" — with `$ref`s already chased.
//!
//! The shape is a *hint*, never a contract. BRP serialises through each type's `Serialize` when it
//! has one, so a `Vec3` is registered as a struct of `x`/`y`/`z` but travels as `[x, y, z]`. An
//! editor must therefore be driven by the *value* it was handed and use the shape only to pick the
//! right control (number vs. integer, which enum variants exist, how to address a field). Anything
//! the schema does not make clear degrades to [`Shape::Json`] — the raw-JSON editor — instead of
//! guessing.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

/// How deep a shape is expanded before giving up and returning [`Shape::Json`]. Deep enough for a
/// component of structs of enums of structs; shallow enough that a self-referential type (a tree
/// node holding a `Vec` of itself) cannot run away even if the cycle guard missed it.
const MAX_DEPTH: u8 = 6;

/// One registered type, as listed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TypeInfo {
    pub type_path: String,
    pub short_path: String,
    pub crate_name: Option<String>,
    /// `reflectTypes` from the schema: `Component`, `Resource`, `Serialize`, `Default`, …
    pub reflect_types: Vec<String>,
    /// `Struct`, `Enum`, `Opaque`, …
    pub kind: Option<String>,
}

impl TypeInfo {
    pub fn is_component(&self) -> bool {
        self.reflect_types.iter().any(|r| r == "Component")
    }
    pub fn is_resource(&self) -> bool {
        self.reflect_types.iter().any(|r| r == "Resource")
    }
}

/// Which registered types a listing wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFilter {
    Any,
    Component,
    Resource,
}

impl TypeFilter {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("component") => TypeFilter::Component,
            Some("resource") => TypeFilter::Resource,
            _ => TypeFilter::Any,
        }
    }
}

/// How a struct-variant / tuple-variant of an enum carries its payload.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VariantShape {
    Unit { name: String },
    Tuple { name: String, items: Vec<Shape> },
    Struct { name: String, fields: Vec<FieldShape> },
}

impl VariantShape {
    pub fn name(&self) -> &str {
        match self {
            VariantShape::Unit { name }
            | VariantShape::Tuple { name, .. }
            | VariantShape::Struct { name, .. } => name,
        }
    }
}

/// A variant plus the placeholder value that selects it — what an editor writes when the user picks
/// this variant from a dropdown, so the frontend never has to know how an enum is spelled on the wire.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VariantEntry {
    #[serde(flatten)]
    pub shape: VariantShape,
    pub sample: Value,
}

impl From<VariantShape> for VariantEntry {
    fn from(shape: VariantShape) -> Self {
        let sample = variant_skeleton(&shape);
        Self { shape, sample }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldShape {
    pub name: String,
    pub shape: Shape,
}

/// What a typed editor needs to know about a type. See the module docs for why this is a hint.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Bool,
    Int,
    Float,
    String,
    /// An `Entity` reference — a number on the wire, but worth its own control (jump to it).
    Entity,
    Struct { type_path: String, fields: Vec<FieldShape> },
    /// A tuple or tuple struct. Addressed by position.
    Tuple { type_path: String, items: Vec<Shape> },
    Enum { type_path: String, variants: Vec<VariantEntry> },
    List { item: Box<Shape> },
    Map { key: Box<Shape>, value: Box<Shape> },
    /// Not described well enough to build a control for: edit it as raw JSON.
    Json,
}

/// The registry, plus the raw schema entries shapes are resolved against.
#[derive(Debug, Default, Clone)]
pub struct TypeRegistry {
    schemas: BTreeMap<String, Value>,
    infos: Vec<TypeInfo>,
}

impl TypeRegistry {
    /// Build from a `registry.schema` response. Tolerates the type map being wrapped one level
    /// down (see [`crate::BrpCapabilities::ingest_schema`]); anything unrecognisable yields an
    /// empty registry rather than an error — the inspector still works without type hints.
    pub fn from_schema(schema: &Value) -> Self {
        let Some(map) = crate::locate_type_map(schema) else {
            return Self::default();
        };
        let mut schemas = BTreeMap::new();
        let mut infos = Vec::new();
        for (path, entry) in map {
            infos.push(info_of(path, entry));
            schemas.insert(path.clone(), entry.clone());
        }
        infos.sort_by(|a, b| a.type_path.cmp(&b.type_path));
        Self { schemas, infos }
    }

    pub fn len(&self) -> usize {
        self.infos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    /// `(components, resources)` registered — for a status line.
    pub fn count_by_kind(&self) -> (usize, usize) {
        (
            self.infos.iter().filter(|i| i.is_component()).count(),
            self.infos.iter().filter(|i| i.is_resource()).count(),
        )
    }

    pub fn info(&self, type_path: &str) -> Option<&TypeInfo> {
        self.infos
            .binary_search_by(|i| i.type_path.as_str().cmp(type_path))
            .ok()
            .map(|ix| &self.infos[ix])
    }

    /// A page of the registered types matching `filter` and `query` (case-insensitive substring of
    /// the full path), best matches first: a short name that *starts* with the query outranks one
    /// that merely contains it. Returns the total matching count and the requested slice.
    pub fn search(
        &self,
        filter: TypeFilter,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> (usize, Vec<&TypeInfo>) {
        let q = query.trim().to_lowercase();
        let mut hits: Vec<(u8, &TypeInfo)> = self
            .infos
            .iter()
            .filter(|i| match filter {
                TypeFilter::Any => true,
                TypeFilter::Component => i.is_component(),
                TypeFilter::Resource => i.is_resource(),
            })
            .filter_map(|i| {
                if q.is_empty() {
                    return Some((1, i));
                }
                let short = i.short_path.to_lowercase();
                if short.starts_with(&q) {
                    Some((0, i))
                } else if i.type_path.to_lowercase().contains(&q) {
                    Some((1, i))
                } else {
                    None
                }
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.type_path.cmp(&b.1.type_path)));
        let total = hits.len();
        let page = hits.into_iter().skip(offset).take(limit).map(|(_, i)| i).collect();
        (total, page)
    }

    /// The editor shape of `type_path`. An unknown path is [`Shape::Json`].
    pub fn shape(&self, type_path: &str) -> Shape {
        self.shape_at(type_path, 0, &mut Vec::new())
    }

    fn shape_at(&self, type_path: &str, depth: u8, stack: &mut Vec<String>) -> Shape {
        if let Some(p) = primitive(type_path) {
            return p;
        }
        if depth >= MAX_DEPTH || stack.iter().any(|s| s == type_path) {
            return Shape::Json;
        }
        let Some(entry) = self.schemas.get(type_path) else {
            return Shape::Json;
        };
        stack.push(type_path.to_string());
        let shape = self.shape_of_entry(type_path, entry, depth, stack);
        stack.pop();
        shape
    }

    fn shape_of_entry(&self, path: &str, entry: &Value, depth: u8, stack: &mut Vec<String>) -> Shape {
        let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("");
        match kind {
            "Struct" => Shape::Struct {
                type_path: path.to_string(),
                fields: self.fields_of(entry, depth, stack),
            },
            "TupleStruct" | "Tuple" => Shape::Tuple {
                type_path: path.to_string(),
                items: self.items_of(entry.get("prefixItems"), depth, stack),
            },
            "Enum" => Shape::Enum {
                type_path: path.to_string(),
                variants: self.variants_of(entry.get("oneOf"), depth, stack),
            },
            "List" | "Array" | "Set" => Shape::List {
                item: Box::new(self.variant_shape(entry.get("items"), depth, stack)),
            },
            "Map" => Shape::Map {
                key: Box::new(self.variant_shape(entry.get("keyType"), depth, stack)),
                value: Box::new(self.variant_shape(entry.get("valueType"), depth, stack)),
            },
            // `Opaque` and anything newer: the JSON-schema `type` is the only clue left.
            _ => match entry.get("type").and_then(Value::as_str) {
                Some("boolean") => Shape::Bool,
                Some("integer") => Shape::Int,
                Some("number") => Shape::Float,
                Some("string") => Shape::String,
                _ => Shape::Json,
            },
        }
    }

    /// The fields of a struct-like schema `holder` (a type entry or a struct variant).
    ///
    /// `properties` is a JSON object and so arrives sorted by key, which loses declaration order —
    /// and order matters, because a `Quat` travels as `[x, y, z, w]` while its fields sort as
    /// `w, x, y, z`. `required` is an array and keeps the order the fields were declared in, so it
    /// decides the order; anything it does not mention follows alphabetically.
    fn fields_of(&self, holder: &Value, depth: u8, stack: &mut Vec<String>) -> Vec<FieldShape> {
        let Some(Value::Object(map)) = holder.get("properties") else { return Vec::new() };
        let declared: Vec<&str> = holder
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut names: Vec<&String> = map.keys().collect();
        names.sort_by_key(|n| declared.iter().position(|d| d == n).unwrap_or(usize::MAX));
        names
            .into_iter()
            .map(|name| FieldShape {
                name: name.clone(),
                shape: self.variant_shape(map.get(name), depth, stack),
            })
            .collect()
    }

    fn items_of(&self, items: Option<&Value>, depth: u8, stack: &mut Vec<String>) -> Vec<Shape> {
        let Some(Value::Array(arr)) = items else { return Vec::new() };
        arr.iter().map(|v| self.variant_shape(Some(v), depth, stack)).collect()
    }

    fn variants_of(&self, one_of: Option<&Value>, depth: u8, stack: &mut Vec<String>) -> Vec<VariantEntry> {
        let Some(Value::Array(arr)) = one_of else { return Vec::new() };
        arr.iter()
            .filter_map(|v| match v {
                Value::String(name) => Some(VariantShape::Unit { name: name.clone() }),
                Value::Object(o) => {
                    let name = o
                        .get("shortPath")
                        .or_else(|| o.get("typePath"))
                        .and_then(Value::as_str)?
                        .to_string();
                    if o.get("properties").is_some() {
                        Some(VariantShape::Struct {
                            name,
                            fields: self.fields_of(v, depth + 1, stack),
                        })
                    } else if o.get("prefixItems").is_some() {
                        Some(VariantShape::Tuple {
                            name,
                            items: self.items_of(o.get("prefixItems"), depth + 1, stack),
                        })
                    } else {
                        Some(VariantShape::Unit { name })
                    }
                }
                _ => None,
            })
            .map(VariantEntry::from)
            .collect()
    }

    /// A property / item slot (`{"type": {"$ref": …}}` and its relatives) → its shape.
    fn variant_shape(&self, slot: Option<&Value>, depth: u8, stack: &mut Vec<String>) -> Shape {
        let Some(slot) = slot else { return Shape::Json };
        match ref_of(slot) {
            Some(path) => self.shape_at(&path, depth + 1, stack),
            None => Shape::Json,
        }
    }
}

fn info_of(path: &str, entry: &Value) -> TypeInfo {
    let str_field = |k: &str| entry.get(k).and_then(Value::as_str).map(str::to_string);
    TypeInfo {
        type_path: path.to_string(),
        short_path: str_field("shortPath")
            .unwrap_or_else(|| path.rsplit("::").next().unwrap_or(path).to_string()),
        crate_name: str_field("crateName"),
        reflect_types: entry
            .get("reflectTypes")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        kind: str_field("kind"),
    }
}

/// The type path a schema slot points at: `{"$ref": "#/$defs/x"}` directly, or under `type`.
fn ref_of(slot: &Value) -> Option<String> {
    let direct = slot.get("$ref").and_then(Value::as_str);
    let nested = slot.get("type").and_then(|t| t.get("$ref")).and_then(Value::as_str);
    direct.or(nested).map(|r| r.trim_start_matches("#/$defs/").to_string())
}

fn primitive(type_path: &str) -> Option<Shape> {
    match type_path {
        "bool" => Some(Shape::Bool),
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128"
        | "isize" => Some(Shape::Int),
        "f32" | "f64" => Some(Shape::Float),
        "alloc::string::String" | "String" | "&str" | "str" | "char" => Some(Shape::String),
        "bevy_ecs::entity::Entity" => Some(Shape::Entity),
        _ => None,
    }
}

/// A well-formed placeholder value for `shape`: what "insert this component" starts from, so the
/// user edits a valid document instead of authoring one from nothing. Not a `Default::default()` —
/// the game may reject it (an enum's first variant is rarely the useful one); it is *shaped*
/// correctly, which is the part that is tedious to type.
pub fn skeleton(shape: &Shape) -> Value {
    match shape {
        Shape::Bool => Value::Bool(false),
        Shape::Int | Shape::Entity => Value::from(0),
        Shape::Float => Value::from(0.0),
        Shape::String => Value::String(String::new()),
        Shape::Struct { fields, .. } => {
            let mut m = Map::new();
            for f in fields {
                m.insert(f.name.clone(), skeleton(&f.shape));
            }
            Value::Object(m)
        }
        Shape::Tuple { items, .. } => match items.as_slice() {
            // A newtype is transparent on the wire.
            [only] => skeleton(only),
            _ => Value::Array(items.iter().map(skeleton).collect()),
        },
        Shape::Enum { variants, .. } => match variants.first() {
            None => Value::Null,
            Some(v) => v.sample.clone(),
        },
        Shape::List { .. } => Value::Array(Vec::new()),
        Shape::Map { .. } => Value::Object(Map::new()),
        Shape::Json => Value::Null,
    }
}

/// The wire form of one enum variant filled with placeholders.
pub fn variant_skeleton(v: &VariantShape) -> Value {
    match v {
        VariantShape::Unit { name } => Value::String(name.clone()),
        VariantShape::Tuple { name, items } => {
            let payload = match items.as_slice() {
                [only] => skeleton(only),
                _ => Value::Array(items.iter().map(skeleton).collect()),
            };
            let mut m = Map::new();
            m.insert(name.clone(), payload);
            Value::Object(m)
        }
        VariantShape::Struct { name, fields } => {
            let mut inner = Map::new();
            for f in fields {
                inner.insert(f.name.clone(), skeleton(&f.shape));
            }
            let mut m = Map::new();
            m.insert(name.clone(), Value::Object(inner));
            Value::Object(m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> TypeRegistry {
        TypeRegistry::from_schema(&json!({
            "my_game::Health": {
                "shortPath": "Health", "typePath": "my_game::Health", "crateName": "my_game",
                "kind": "Struct", "type": "object",
                "reflectTypes": ["Component", "Default"],
                "required": ["current", "max", "label", "mode"],
                "properties": {
                    "current": { "type": { "$ref": "#/$defs/f32" } },
                    "max": { "type": { "$ref": "#/$defs/f32" } },
                    "label": { "type": { "$ref": "#/$defs/alloc::string::String" } },
                    "mode": { "type": { "$ref": "#/$defs/my_game::Mode" } }
                }
            },
            "my_game::Mode": {
                "shortPath": "Mode", "typePath": "my_game::Mode", "kind": "Enum",
                "reflectTypes": [],
                "oneOf": [
                    "Idle",
                    { "shortPath": "Moving", "kind": "Tuple", "prefixItems": [ { "type": { "$ref": "#/$defs/f32" } } ] },
                    { "shortPath": "Aiming", "properties": { "angle": { "type": { "$ref": "#/$defs/f32" } } } }
                ]
            },
            "my_game::Score": {
                "shortPath": "Score", "typePath": "my_game::Score", "kind": "Struct",
                "reflectTypes": ["Resource"],
                "properties": { "points": { "type": { "$ref": "#/$defs/u32" } } }
            },
            "my_game::Tree": {
                "shortPath": "Tree", "typePath": "my_game::Tree", "kind": "Struct",
                "reflectTypes": ["Component"],
                "properties": { "child": { "type": { "$ref": "#/$defs/my_game::Tree" } } }
            }
        }))
    }

    #[test]
    fn lists_by_reflect_kind() {
        let r = registry();
        let (n, comps) = r.search(TypeFilter::Component, "", 0, 10);
        assert_eq!(n, 2);
        assert_eq!(comps[0].type_path, "my_game::Health");
        let (n, res) = r.search(TypeFilter::Resource, "", 0, 10);
        assert_eq!((n, res[0].short_path.as_str()), (1, "Score"));
    }

    #[test]
    fn search_ranks_short_name_prefix_first_and_pages() {
        let r = registry();
        let (n, hits) = r.search(TypeFilter::Any, "he", 0, 10);
        assert_eq!(n, 1);
        assert_eq!(hits[0].short_path, "Health");
        let (n, page) = r.search(TypeFilter::Any, "my_game", 1, 2);
        assert_eq!(n, 4);
        assert_eq!(page.len(), 2);
    }

    #[test]
    fn struct_shape_chases_refs_into_primitives_and_enums() {
        let Shape::Struct { fields, .. } = registry().shape("my_game::Health") else {
            panic!("expected a struct");
        };
        assert_eq!(fields[0].name, "current");
        assert_eq!(fields[0].shape, Shape::Float);
        assert_eq!(fields[1].shape, Shape::Float);
        assert_eq!(fields[2].shape, Shape::String);
        let Shape::Enum { variants, .. } = &fields[3].shape else { panic!("expected an enum") };
        assert_eq!(variants.len(), 3);
        assert!(matches!(variants[1].shape, VariantShape::Tuple { .. }));
        assert!(matches!(variants[2].shape, VariantShape::Struct { .. }));
        assert_eq!(variants[1].sample, json!({ "Moving": 0.0 }));
    }

    #[test]
    fn self_referential_type_terminates() {
        let Shape::Struct { fields, .. } = registry().shape("my_game::Tree") else {
            panic!("expected a struct");
        };
        // The cycle is cut at the first repeat, not expanded forever.
        assert_eq!(fields[0].shape, Shape::Json);
    }

    #[test]
    fn unknown_type_is_raw_json() {
        assert_eq!(registry().shape("nope::Missing"), Shape::Json);
    }

    #[test]
    fn skeleton_is_a_well_formed_document() {
        let r = registry();
        let v = skeleton(&r.shape("my_game::Health"));
        assert_eq!(v["current"], json!(0.0));
        assert_eq!(v["label"], json!(""));
        // First variant of the enum: a unit one, so a bare string.
        assert_eq!(v["mode"], json!("Idle"));
    }

    #[test]
    fn variant_skeletons_follow_the_wire_form() {
        let tuple = VariantShape::Tuple { name: "Moving".into(), items: vec![Shape::Float] };
        assert_eq!(variant_skeleton(&tuple), json!({ "Moving": 0.0 }));
        let strukt = VariantShape::Struct {
            name: "Aiming".into(),
            fields: vec![FieldShape { name: "angle".into(), shape: Shape::Float }],
        };
        assert_eq!(variant_skeleton(&strukt), json!({ "Aiming": { "angle": 0.0 } }));
    }

    #[test]
    fn unrecognisable_schema_gives_an_empty_registry() {
        assert!(TypeRegistry::from_schema(&json!({ "nope": 1 })).is_empty());
    }
}
