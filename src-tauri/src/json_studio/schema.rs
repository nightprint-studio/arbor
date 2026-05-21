//! JSON Schema → studio schema bridge for JSON Studio.
//!
//! Phase 3.c.b feature. Lets the user point JSON Studio at a JSON
//! Schema file (`*.schema.json`, draft-04 / 07 / 2020-12 — we don't
//! gate on the `$schema` keyword) and surface the same schema sidebar
//! RON Studio offers for Rust crates: a root-type picker, resolved
//! fields with type badges, a reachable-types coverage list.
//!
//! Mapping JSON Schema → unified `Schema` (`ron_studio::schema::{Schema,
//! TypeDef, ResolvedType, ...}`):
//!   · `type: object` with `properties` → `TypeDef::Struct`.
//!   · `oneOf` / `anyOf` of string `const`s              → `TypeDef::Enum`
//!     with unit variants (named after the const value).
//!   · Everything else (top-level alias, `$ref` chains, raw scalar)
//!     → `TypeDef::Alias` whose target is the resolved type.
//!
//! Field type resolution:
//!   · `$ref: "#/$defs/X"` (or `#/definitions/X`)         → `Named { path }`
//!   · `$ref` to URI                                      → `External { path }`
//!   · `type: "string" | "number" | "integer" | "boolean" | "null"`
//!                                                         → `Primitive`
//!   · `type: "array"` with `items: T`                    → `Vec { inner }`
//!   · `type: "object"` with `additionalProperties: T`    → `Map { string, T }`
//!   · `enum: [...]` strings only                         → `Primitive { string }`
//!   · `oneOf`/`anyOf` / no `type` keyword                → `Unknown`
//!
//! The "canonical path" of a definition is a JSON Pointer string
//! rooted at the schema file: `#` for the root, `#/$defs/Foo` for
//! `$defs.Foo`, `#/definitions/Foo` for the legacy keyword. That
//! mirrors what `$ref` keywords use, so `view_source` can resolve any
//! pointer the FE shows back to its location in the schema text.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::PathBuf;

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::ron_studio::schema::{
    CandidateKind, CrateProbe, FieldDef, ResolvedType, RootCandidate, Schema,
    SchemaStats, TypeDef, TypeSource,
};

// ── Public entry points ──────────────────────────────────────────────────────

/// Stage 1 — open the JSON Schema file and list every named definition
/// the user can pick as a root. Always includes a synthetic `#` entry
/// for the root schema (the file's top-level shape), plus one entry
/// per key in `$defs` / `definitions`.
pub fn probe(schema_file_path: &str) -> Result<CrateProbe> {
    let path = PathBuf::from(schema_file_path);
    if !path.is_file() {
        return Err(AppError::Other(format!(
            "Not a file: {schema_file_path}",
        )));
    }
    let text = read_file_text(&path)?;
    let schema: Value = serde_json::from_str(&text).map_err(|e| {
        AppError::Other(format!("JSON Schema parse error in {schema_file_path}: {e}"))
    })?;

    let schema_label = schema_label_for(&schema, &path);
    let mut candidates: Vec<RootCandidate> = Vec::new();

    // Root candidate (`#`). Use the schema's `title` when present,
    // otherwise the file basename.
    candidates.push(RootCandidate {
        name:           schema_title(&schema)
            .unwrap_or_else(|| file_stem(&path).unwrap_or_else(|| "(root)".into())),
        canonical_path: "#".into(),
        kind:           candidate_kind_for(&schema),
    });

    for (key, defs) in iter_def_buckets(&schema) {
        if let Value::Object(map) = defs {
            for (name, def_value) in map {
                let pointer = format!("#/{key}/{}", encode_pointer_token(name));
                candidates.push(RootCandidate {
                    name:           schema_title(def_value).unwrap_or_else(|| name.clone()),
                    canonical_path: pointer,
                    kind:           candidate_kind_for(def_value),
                });
            }
        }
    }

    Ok(CrateProbe {
        crate_manifest:  schema_file_path.to_string(),
        crate_name:      schema_label,
        root_candidates: candidates,
    })
}

/// Stage 2 — fully resolve the schema rooted at `root_canonical_path`
/// into a `Schema`. Walks every reachable `$ref` and indexes the
/// definitions it finds. Cycles are tolerated (we visit each canonical
/// path at most once).
pub fn load(schema_file_path: &str, root_canonical_path: &str) -> Result<Schema> {
    let path = PathBuf::from(schema_file_path);
    let text = read_file_text(&path)?;
    let schema: Value = serde_json::from_str(&text).map_err(|e| {
        AppError::Other(format!("JSON Schema parse error in {schema_file_path}: {e}"))
    })?;

    let mut types:   BTreeMap<String, TypeDef> = BTreeMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue:   VecDeque<String> = VecDeque::new();
    queue.push_back(root_canonical_path.to_string());

    while let Some(canonical) = queue.pop_front() {
        if !visited.insert(canonical.clone()) { continue; }
        let Some(def_value) = resolve_pointer(&schema, &canonical) else { continue; };
        let def = build_type_def(&canonical, def_value, &mut queue);
        types.insert(canonical, def);
    }

    let root_name = match resolve_pointer(&schema, root_canonical_path) {
        Some(v) => schema_title(v).unwrap_or_else(|| last_segment(root_canonical_path)),
        None    => last_segment(root_canonical_path),
    };
    let schema_label = schema_label_for(&schema, &path);

    let stats = compute_stats(&types);
    Ok(Schema {
        root_type:      root_canonical_path.to_string(),
        root_name,
        crate_manifest: schema_file_path.to_string(),
        crate_name:     schema_label,
        types,
        stats,
    })
}

/// Pretty-print the JSON Schema fragment at `canonical_path`. Used by
/// the "View implementation" modal (same UX RON Studio uses for crate
/// types).
pub fn get_type_source(schema_file_path: &str, canonical_path: &str) -> Result<TypeSource> {
    let path = PathBuf::from(schema_file_path);
    let text = read_file_text(&path)?;
    let schema: Value = serde_json::from_str(&text).map_err(|e| {
        AppError::Other(format!("JSON Schema parse error in {schema_file_path}: {e}"))
    })?;
    let def_value = resolve_pointer(&schema, canonical_path).ok_or_else(|| {
        AppError::Other(format!("Definition not found in schema: {canonical_path}"))
    })?;
    let source = serde_json::to_string_pretty(def_value).map_err(|e| {
        AppError::Other(format!("Pretty-print {canonical_path}: {e}"))
    })?;
    let name = schema_title(def_value).unwrap_or_else(|| last_segment(canonical_path));
    Ok(TypeSource {
        canonical_path: canonical_path.to_string(),
        name,
        kind: candidate_kind_for(def_value),
        source,
    })
}

// ── Type-def construction ────────────────────────────────────────────────────

fn build_type_def(canonical: &str, v: &Value, queue: &mut VecDeque<String>) -> TypeDef {
    let name = schema_title(v).unwrap_or_else(|| last_segment(canonical));

    // Object with `properties` → Struct.
    if value_is_object_schema(v) {
        let props = v.get("properties").and_then(Value::as_object);
        let required: HashSet<&str> = v.get("required")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let fields: Vec<FieldDef> = match props {
            Some(map) => map.iter().map(|(fname, fval)| {
                let ty = resolve_ref_or_type(fval, queue);
                let req = required.contains(fname.as_str());
                FieldDef {
                    name:            fname.clone(),
                    ty,
                    aliases:         Vec::new(),
                    has_default:     !req || fval.get("default").is_some(),
                    skip_if_default: false,
                    flatten:         false,
                }
            }).collect(),
            None => Vec::new(),
        };
        return TypeDef::Struct {
            name,
            fields,
            tuple_like: false,
        };
    }

    // String enum (`enum: ["a", "b"]` or `oneOf: [{const: "a"}, ...]`)
    // → Enum with unit variants. Numeric / mixed enums fall through to
    // the alias branch.
    if let Some(variants) = enum_variants(v) {
        return TypeDef::Enum {
            name,
            variants,
        };
    }

    // Anything else → alias to the resolved type expression.
    TypeDef::Alias {
        name,
        target: resolve_ref_or_type(v, queue),
    }
}

fn value_is_object_schema(v: &Value) -> bool {
    if !matches!(v.get("type"), Some(Value::String(s)) if s == "object") {
        // Fall back: any schema with a properties map counts.
        return v.get("properties").is_some();
    }
    v.get("properties").is_some() || v.get("additionalProperties").is_some()
        || v.get("required").is_some()
}

fn enum_variants(v: &Value) -> Option<Vec<crate::ron_studio::schema::VariantDef>> {
    use crate::ron_studio::schema::{VariantDef, VariantShape};

    // Direct `enum: [strings]`
    if let Some(values) = v.get("enum").and_then(Value::as_array) {
        let names: Option<Vec<String>> = values.iter()
            .map(|x| x.as_str().map(|s| s.to_string()))
            .collect();
        if let Some(names) = names {
            return Some(names.into_iter().map(|n| VariantDef {
                name: n,
                shape: VariantShape::Unit,
                fields: Vec::new(),
            }).collect());
        }
    }

    // `oneOf` / `anyOf` of `{const: "..."}`
    for key in ["oneOf", "anyOf"] {
        if let Some(arr) = v.get(key).and_then(Value::as_array) {
            let names: Option<Vec<String>> = arr.iter()
                .map(|x| x.get("const").and_then(Value::as_str).map(|s| s.to_string()))
                .collect();
            if let Some(names) = names {
                if !names.is_empty() {
                    return Some(names.into_iter().map(|n| VariantDef {
                        name: n,
                        shape: VariantShape::Unit,
                        fields: Vec::new(),
                    }).collect());
                }
            }
        }
    }
    None
}

fn resolve_ref_or_type(v: &Value, queue: &mut VecDeque<String>) -> ResolvedType {
    // `$ref` is the canonical shortcut. Local refs (starting with `#`)
    // queue up the target for indexing; everything else (external URI)
    // becomes External so the UI shows a muted "external" badge.
    if let Some(target) = v.get("$ref").and_then(Value::as_str) {
        if target.starts_with('#') {
            queue.push_back(target.to_string());
            return ResolvedType::Named { path: target.to_string() };
        }
        return ResolvedType::External { path: target.to_string() };
    }

    let type_value = v.get("type");
    let single_type = match type_value {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    };

    match single_type {
        Some("string")  => ResolvedType::Primitive { name: "string".into() },
        Some("number")  => ResolvedType::Primitive { name: "number".into() },
        Some("integer") => ResolvedType::Primitive { name: "integer".into() },
        Some("boolean") => ResolvedType::Primitive { name: "boolean".into() },
        Some("null")    => ResolvedType::Primitive { name: "null".into() },
        Some("array")   => {
            let inner = match v.get("items") {
                Some(items) => resolve_ref_or_type(items, queue),
                None => ResolvedType::Unknown { hint: "array items unspecified".into() },
            };
            ResolvedType::Vec { inner: Box::new(inner) }
        }
        Some("object") => {
            // If the object declares properties it's named-struct shaped
            // but the user hasn't given us a name to look up. Fall back
            // to a map of any → any if `additionalProperties` is set,
            // or Unknown otherwise.
            if let Some(add) = v.get("additionalProperties") {
                let value_ty = match add {
                    Value::Bool(true) | Value::Null => ResolvedType::Unknown { hint: "any".into() },
                    Value::Bool(false) => ResolvedType::Unknown { hint: "forbidden".into() },
                    obj => resolve_ref_or_type(obj, queue),
                };
                return ResolvedType::Map {
                    key:   Box::new(ResolvedType::Primitive { name: "string".into() }),
                    value: Box::new(value_ty),
                };
            }
            if v.get("properties").is_some() {
                return ResolvedType::Unknown { hint: "inline object".into() };
            }
            ResolvedType::Unknown { hint: "object".into() }
        }
        Some(other) => ResolvedType::Unknown { hint: other.into() },
        None => {
            // No `type` keyword. Handle the common no-type composites.
            if v.get("oneOf").is_some() || v.get("anyOf").is_some() || v.get("allOf").is_some() {
                return ResolvedType::Unknown { hint: "oneOf/anyOf/allOf".into() };
            }
            if let Some(values) = v.get("enum").and_then(Value::as_array) {
                let all_strings = values.iter().all(|x| x.is_string());
                if all_strings {
                    return ResolvedType::Primitive { name: "string".into() };
                }
                return ResolvedType::Unknown { hint: "mixed-type enum".into() };
            }
            ResolvedType::Unknown { hint: "any".into() }
        }
    }
}

// ── Pointer + schema utilities ───────────────────────────────────────────────

fn resolve_pointer<'a>(schema: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer == "#" || pointer.is_empty() {
        return Some(schema);
    }
    let body = pointer.strip_prefix("#/")?;
    let mut cur = schema;
    for raw in body.split('/') {
        let key = decode_pointer_token(raw);
        cur = match cur {
            Value::Object(map) => map.get(&key)?,
            Value::Array(arr) => {
                let idx: usize = key.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

fn iter_def_buckets(schema: &Value) -> impl Iterator<Item = (&'static str, &Value)> {
    ["$defs", "definitions"].into_iter().filter_map(move |key| {
        schema.get(key).map(|v| (key, v))
    })
}

fn schema_title(v: &Value) -> Option<String> {
    v.get("title").and_then(Value::as_str).map(|s| s.to_string())
}

fn schema_label_for(schema: &Value, path: &std::path::Path) -> String {
    if let Some(id) = schema.get("$id").and_then(Value::as_str) {
        return id.to_string();
    }
    if let Some(t) = schema_title(schema) {
        return t;
    }
    file_stem(path).unwrap_or_else(|| path.display().to_string())
}

fn file_stem(path: &std::path::Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
}

fn candidate_kind_for(v: &Value) -> CandidateKind {
    if enum_variants(v).is_some() {
        return CandidateKind::Enum;
    }
    CandidateKind::Struct
}

fn last_segment(canonical: &str) -> String {
    if canonical == "#" || canonical.is_empty() {
        return "root".into();
    }
    let body = canonical.strip_prefix("#/").unwrap_or(canonical);
    body.rsplit('/').next().map(|s| decode_pointer_token(s)).unwrap_or_else(|| canonical.to_string())
}

fn encode_pointer_token(s: &str) -> String {
    // RFC 6901 — escape `~` then `/`.
    s.replace('~', "~0").replace('/', "~1")
}

fn decode_pointer_token(s: &str) -> String {
    // Decode `~1` → `/` first to match the encoder's reverse order.
    s.replace("~1", "/").replace("~0", "~")
}

fn read_file_text(path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| {
        AppError::Other(format!("Read {}: {e}", path.display()))
    })?;
    let (text, _, _) = crate::git::encoding::decode_bytes_full(&bytes);
    Ok(text)
}

fn compute_stats(types: &BTreeMap<String, TypeDef>) -> SchemaStats {
    let mut resolved = 0usize;
    let mut external = 0usize;
    let mut unknown  = 0usize;
    let mut seen_resolved: BTreeSet<&str> = BTreeSet::new();
    for canonical in types.keys() {
        seen_resolved.insert(canonical.as_str());
    }
    for def in types.values() {
        walk_types(def, &seen_resolved, &mut resolved, &mut external, &mut unknown);
    }
    // The top-level type count itself is the "resolved" count for
    // schema.types; track it explicitly so the UI shows the same totals
    // RON does.
    let resolved_total = types.len() + resolved;
    SchemaStats {
        resolved: resolved_total,
        external,
        unknown,
    }
}

fn walk_types(
    def:           &TypeDef,
    seen_resolved: &BTreeSet<&str>,
    resolved:      &mut usize,
    external:      &mut usize,
    unknown:       &mut usize,
) {
    match def {
        TypeDef::Struct { fields, .. } => {
            for f in fields {
                tally_resolved(&f.ty, seen_resolved, resolved, external, unknown);
            }
        }
        TypeDef::Enum { variants, .. } => {
            for v in variants {
                for f in &v.fields {
                    tally_resolved(&f.ty, seen_resolved, resolved, external, unknown);
                }
            }
        }
        TypeDef::Alias { target, .. } => {
            tally_resolved(target, seen_resolved, resolved, external, unknown);
        }
    }
}

fn tally_resolved(
    ty:            &ResolvedType,
    seen_resolved: &BTreeSet<&str>,
    _resolved:     &mut usize,
    external:      &mut usize,
    unknown:       &mut usize,
) {
    match ty {
        ResolvedType::Primitive { .. } => {}
        ResolvedType::Option { inner } | ResolvedType::Vec { inner } => {
            tally_resolved(inner, seen_resolved, _resolved, external, unknown);
        }
        ResolvedType::Map { key, value } => {
            tally_resolved(key,   seen_resolved, _resolved, external, unknown);
            tally_resolved(value, seen_resolved, _resolved, external, unknown);
        }
        ResolvedType::Tuple { items } => {
            for it in items {
                tally_resolved(it, seen_resolved, _resolved, external, unknown);
            }
        }
        ResolvedType::Named { path } => {
            if !seen_resolved.contains(path.as_str()) {
                *external += 1;
            }
        }
        ResolvedType::External { .. } => { *external += 1; }
        ResolvedType::Unknown  { .. } => { *unknown  += 1; }
    }
}
