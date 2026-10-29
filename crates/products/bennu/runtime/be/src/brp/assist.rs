//! What the inspector's handlers do not offer and the assistant needs: **components with their values, for
//! many entities at once**.
//!
//! The inspector reads values for the one selected entity and names for everything else; an assistant
//! asking "which enemies are below 20 health, and where are they" wants the values of a few components
//! across the entities that have them, in one round-trip and one compact answer. That is one `world.query`
//! with the requested components as `components` and `Name` as an `option`; this file builds it, resolves the
//! names the model wrote (`Health` rather than `my_game::combat::Health`) against the registry, and shapes
//! the reply.
//!
//! It adds a handler and nothing else: the connection, the method dialects and the reconnect logic are the
//! ones in `session.rs`, used as they are.

use arbor_brp::prelude::{parse_entity_ref, Op, TypeFilter, TypeRegistry};
use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::{call, live};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;
/// The longest a single component value is shown. A value past it is cut and says how long it was.
const MAX_VALUE_CHARS: usize = 400;

/// Args for [`bennu_brp_query`].
#[derive(Deserialize)]
pub struct QueryArgs {
    /// Components whose values to return — full type paths or short names (`Health`).
    #[serde(default)]
    pub components: Vec<String>,
    /// Only entities that also have all of these (same naming), without returning their values.
    #[serde(default)]
    pub with: Vec<String>,
    /// Only entities that have none of these.
    #[serde(default)]
    pub without: Vec<String>,
    /// Case-insensitive substring of the entity's `Name`, or an entity id.
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Components with values for the entities having them, as the assistant reads it:
/// `{ total, shown, rows: [{ entity, name?, components: { short name: value } }] }`.
#[arbor_rpc::handler]
async fn bennu_brp_query(_ctx: &BennuState, args: QueryArgs) -> Result<Value, String> {
    let live = live()?;
    if args.components.is_empty() && args.with.is_empty() && args.search.as_deref().is_none_or(|s| s.trim().is_empty()) {
        return Err("Say what to look for: `components` (values are returned), `with`, or a name `search`.".into());
    }
    let wanted = resolve_all(&live.registry, &args.components)?;
    let with = resolve_all(&live.registry, &args.with)?;
    let without = resolve_all(&live.registry, &args.without)?;

    let mut data = Map::new();
    if !wanted.is_empty() {
        data.insert("components".into(), json!(wanted));
    }
    if !live.name_types.is_empty() {
        data.insert("option".into(), json!(live.name_types));
    }
    let mut filter = Map::new();
    if !with.is_empty() {
        filter.insert("with".into(), json!(with));
    }
    if !without.is_empty() {
        filter.insert("without".into(), json!(without));
    }
    let raw = call(Op::Query, Some(json!({ "data": data, "filter": filter, "strict": false }))).await?;

    let search = args.search.unwrap_or_default();
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let name_types = live.name_types.clone();
    let rows = tokio::task::spawn_blocking(move || shape_rows(&raw, &name_types, &wanted, &search, limit))
        .await
        .map_err(|e| format!("query failed: {e}"))?;
    Ok(rows)
}

// ── Pure parts ────────────────────────────────────────────────────────────────

/// Every name of `names` as a full type path of a registered component.
fn resolve_all(registry: &TypeRegistry, names: &[String]) -> Result<Vec<String>, String> {
    names.iter().map(|n| resolve_component(registry, n, TypeFilter::Component)).collect()
}

/// Args for [`bennu_brp_resolve`].
#[derive(Deserialize)]
pub struct ResolveArgs {
    pub name: String,
    /// `component` (default) or `resource`.
    #[serde(default)]
    pub kind: Option<String>,
}

/// The full type path a model-written name stands for — so the write tools accept `Health` and `Score`
/// the way the query does, instead of making the model look the path up first.
#[arbor_rpc::handler]
async fn bennu_brp_resolve(_ctx: &BennuState, args: ResolveArgs) -> Result<String, String> {
    let filter = match TypeFilter::parse(args.kind.as_deref()) {
        TypeFilter::Resource => TypeFilter::Resource,
        _ => TypeFilter::Component,
    };
    resolve_component(&live()?.registry, &args.name, filter)
}

/// `Health` → `my_game::combat::Health`. A full path is taken as it is when registered; a short name
/// must match one component — an ambiguous one is refused with the candidates, because guessing which
/// `Health` the model meant would quietly answer a different question.
fn resolve_component(registry: &TypeRegistry, name: &str, filter: TypeFilter) -> Result<String, String> {
    let name = name.trim();
    if registry.is_empty() || registry.info(name).is_some() {
        // A game that withheld its registry cannot be checked: the name goes to it as written.
        return Ok(name.to_string());
    }
    let (_, hits) = registry.search(filter, name, 0, 8);
    let exact: Vec<_> = hits.iter().filter(|i| i.short_path.eq_ignore_ascii_case(name)).collect();
    match (exact.as_slice(), hits.as_slice()) {
        ([one], _) => Ok(one.type_path.clone()),
        ([], [one]) => Ok(one.type_path.clone()),
        ([], []) => Err(format!("No registered component matches `{name}`. Use brp_types to search the registry.")),
        (several, rest) => {
            let list: Vec<&str> = if several.is_empty() { rest.iter().map(|i| i.type_path.as_str()).collect() } else { several.iter().map(|i| i.type_path.as_str()).collect() };
            Err(format!("`{name}` is ambiguous: {}. Give the full type path.", list.join(", ")))
        }
    }
}

/// The rows of a `world.query` answer that match `search`, at most `limit` of them, with the values cut to
/// a readable size and keyed by short type name.
fn shape_rows(raw: &Value, name_types: &[String], wanted: &[String], search: &str, limit: usize) -> Value {
    let needle = search.trim().to_lowercase();
    let mut total = 0usize;
    let mut rows: Vec<Value> = Vec::new();
    for item in raw.as_array().into_iter().flatten() {
        let Some(entity) = item.get("entity").and_then(parse_entity_ref) else { continue };
        let components = item.get("components").and_then(Value::as_object);
        let name = components.and_then(|c| name_types.iter().find_map(|t| c.get(t))).and_then(name_of);
        if !needle.is_empty() && !matches_search(entity, name.as_deref(), &needle) {
            continue;
        }
        total += 1;
        if rows.len() >= limit {
            continue;
        }
        let mut values = Map::new();
        for path in wanted {
            if let Some(v) = components.and_then(|c| c.get(path)) {
                values.insert(arbor_brp::prelude::short_type_name(path), shorten(v));
            }
        }
        let mut row = json!({ "entity": entity });
        if let Some(n) = name {
            row["name"] = json!(n);
        }
        if !values.is_empty() {
            row["components"] = Value::Object(values);
        }
        rows.push(row);
    }
    json!({ "total": total, "shown": rows.len(), "rows": rows })
}

fn matches_search(entity: u64, name: Option<&str>, needle: &str) -> bool {
    name.is_some_and(|n| n.to_lowercase().contains(needle))
        || (needle.chars().all(|c| c.is_ascii_digit()) && entity.to_string() == needle)
}

/// A `Name` component's text: a bare string, or a struct with a `name` field.
fn name_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("name").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// `v` as it is, or — when its JSON is longer than a screenful — a string saying how long and showing the start.
fn shorten(v: &Value) -> Value {
    let text = v.to_string();
    let len = text.chars().count();
    if len <= MAX_VALUE_CHARS {
        return v.clone();
    }
    let head: String = text.chars().take(MAX_VALUE_CHARS).collect();
    Value::String(format!("{head}… ({len} chars, cut)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> TypeRegistry {
        let entry = |path: &str, short: &str| {
            json!({ "shortPath": short, "typePath": path, "kind": "Struct", "reflectTypes": ["Component"], "properties": {} })
        };
        TypeRegistry::from_schema(&json!({
            "my_game::combat::Health": entry("my_game::combat::Health", "Health"),
            "my_game::ui::Health": entry("my_game::ui::Health", "Health"),
            "my_game::Player": entry("my_game::Player", "Player"),
            "my_game::PlayerStats": entry("my_game::PlayerStats", "PlayerStats"),
            "my_game::Velocity": entry("my_game::Velocity", "Velocity"),
        }))
    }

    #[test]
    fn a_short_name_resolves_to_the_one_component_it_names() {
        let r = registry();
        assert_eq!(resolve_component(&r, "Velocity", TypeFilter::Component).unwrap(), "my_game::Velocity");
        assert_eq!(resolve_component(&r, "my_game::combat::Health", TypeFilter::Component).unwrap(), "my_game::combat::Health");
        // `Player` is also a prefix of `PlayerStats`, but the exact short name wins.
        assert_eq!(resolve_component(&r, "player", TypeFilter::Component).unwrap(), "my_game::Player");
    }

    #[test]
    fn an_ambiguous_or_unknown_name_is_refused_with_help() {
        let r = registry();
        let ambiguous = resolve_component(&r, "Health", TypeFilter::Component).unwrap_err();
        assert!(ambiguous.contains("ambiguous") && ambiguous.contains("my_game::ui::Health"), "{ambiguous}");
        assert!(resolve_component(&r, "Nope", TypeFilter::Component).unwrap_err().contains("brp_types"));
    }

    #[test]
    fn a_game_without_a_registry_gets_the_name_as_written() {
        assert_eq!(resolve_component(&TypeRegistry::default(), " Health ", TypeFilter::Component).unwrap(), "Health");
    }

    fn answer() -> Value {
        json!([
            { "entity": 4294967296u64, "components": { "bevy_ecs::name::Name": "Goblin", "my_game::Velocity": { "x": 1.0, "y": 0.0 } } },
            { "entity": 7, "components": { "bevy_ecs::name::Name": "Orc", "my_game::Velocity": { "x": 0.0, "y": 2.0 } } },
            { "entity": 9, "components": { "my_game::Velocity": { "x": 3.0, "y": 3.0 } } },
        ])
    }

    const NAME: &str = "bevy_ecs::name::Name";
    const VELOCITY: &str = "my_game::Velocity";

    #[test]
    fn rows_carry_the_name_and_the_values_under_their_short_name() {
        let out = shape_rows(&answer(), &[NAME.into()], &[VELOCITY.into()], "", 10);
        assert_eq!((out["total"].as_u64(), out["shown"].as_u64()), (Some(3), Some(3)));
        assert_eq!(out["rows"][0]["name"], "Goblin");
        assert_eq!(out["rows"][0]["components"]["Velocity"]["x"], 1.0);
        assert!(out["rows"][2].get("name").is_none(), "an unnamed entity has no name");
    }

    #[test]
    fn the_limit_caps_the_rows_but_not_the_count() {
        let out = shape_rows(&answer(), &[NAME.into()], &[VELOCITY.into()], "", 2);
        assert_eq!((out["total"].as_u64(), out["shown"].as_u64()), (Some(3), Some(2)));
    }

    #[test]
    fn a_search_matches_a_name_or_an_entity_id() {
        let by_name = shape_rows(&answer(), &[NAME.into()], &[VELOCITY.into()], "ORC", 10);
        assert_eq!(by_name["total"], 1);
        let by_id = shape_rows(&answer(), &[NAME.into()], &[VELOCITY.into()], "9", 10);
        assert_eq!(by_id["rows"][0]["entity"], 9);
    }

    #[test]
    fn a_huge_value_is_cut_and_says_how_long_it_was() {
        let big = json!({ "items": "x".repeat(2_000) });
        let cut = shorten(&big);
        assert!(cut.as_str().unwrap().contains("chars, cut"));
        assert_eq!(shorten(&json!({"a": 1})), json!({"a": 1}));
    }
}
