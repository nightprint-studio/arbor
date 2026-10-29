//! Reading a game's world: the entity snapshot a tree is served from, and the response envelopes
//! that changed shape between Bevy releases.
//!
//! BRP has no pagination and no way to ask "entities named X". The cheapest question it *can* be
//! asked is a `query` for nothing but the `Name` and parent components of every entity — and that
//! is what a [`Snapshot`] is built from. Everything a UI then does (expand a node, search by name,
//! scroll a page) is answered from the snapshot in memory, so the game is asked **once per
//! refresh**, not once per row on screen. Per-entity component values are a different, per-selection
//! call and never part of the snapshot.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use serde::Serialize;
use serde_json::{Map, Value};

/// One entity, as light as the tree needs it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityRow {
    pub id: u64,
    pub name: Option<String>,
    pub parent: Option<u64>,
}

/// A row as served to a UI: the entity plus what a tree needs to draw its expander.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RowView {
    pub id: u64,
    pub name: Option<String>,
    pub parent: Option<u64>,
    pub child_count: usize,
}

/// What a UI asks of the snapshot.
#[derive(Debug, Clone, Default)]
pub struct PageQuery<'a> {
    /// Children of this entity. Ignored in flat mode (see [`PageQuery::is_flat`]).
    pub parent: Option<u64>,
    /// Case-insensitive substring of the name, or digits matching the entity's id / index.
    pub search: &'a str,
    /// When set, only these entities — the result of a component-presence query to the game.
    pub only: Option<&'a HashSet<u64>>,
    pub offset: usize,
    pub limit: usize,
}

impl PageQuery<'_> {
    /// A search or a component filter flattens the tree: a hit deep in the hierarchy has to be
    /// shown, and showing it under collapsed ancestors would hide it.
    pub fn is_flat(&self) -> bool {
        !self.search.trim().is_empty() || self.only.is_some()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Page {
    pub total: usize,
    pub rows: Vec<RowView>,
}

/// The world's entities at one instant, indexed for tree browsing.
#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    rows: Vec<EntityRow>,
    by_id: HashMap<u64, usize>,
    children: HashMap<u64, Vec<u64>>,
    revision: u64,
}

impl Snapshot {
    pub fn build(mut rows: Vec<EntityRow>) -> Self {
        rows.sort_by_key(|r| r.id);
        rows.dedup_by_key(|r| r.id);
        let by_id: HashMap<u64, usize> = rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        for r in &rows {
            if let Some(p) = r.parent {
                children.entry(p).or_default().push(r.id);
            }
        }
        // `DefaultHasher::new()` is keyed with constants, so equal worlds hash equal across calls —
        // that equality is the whole point: "did anything change since last time".
        let mut h = std::collections::hash_map::DefaultHasher::new();
        rows.hash(&mut h);
        Self { rows, by_id, children, revision: h.finish() }
    }

    /// Build from a `world.query` result that asked for the name and parent components as
    /// `option`s. `name_types` / `parent_types` are the registered paths for those components.
    pub fn from_query(result: &Value, name_types: &[String], parent_types: &[String]) -> Self {
        let Some(items) = result.as_array() else { return Self::default() };
        let rows = items
            .iter()
            .filter_map(|item| {
                let id = item.get("entity").and_then(parse_entity_ref)?;
                let comps = item.get("components").and_then(Value::as_object);
                let find = |paths: &[String]| -> Option<&Value> {
                    let comps = comps?;
                    paths.iter().find_map(|p| comps.get(p))
                };
                Some(EntityRow {
                    id,
                    name: find(name_types).and_then(parse_name),
                    parent: find(parent_types).and_then(parse_entity_ref),
                })
            })
            .collect();
        Self::build(rows)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Changes whenever any entity appears, disappears, is renamed or is reparented.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, id: u64) -> Option<&EntityRow> {
        self.by_id.get(&id).map(|&i| &self.rows[i])
    }

    fn view(&self, r: &EntityRow) -> RowView {
        RowView {
            id: r.id,
            name: r.name.clone(),
            parent: r.parent,
            child_count: self.children.get(&r.id).map_or(0, Vec::len),
        }
    }

    /// A page of the snapshot. Tree mode (`parent`) lists one level; flat mode lists every match.
    /// A root is an entity with no parent **or** whose parent is not in the snapshot — a stale
    /// reference must not make a subtree unreachable.
    pub fn page(&self, q: &PageQuery<'_>) -> Page {
        let flat = q.is_flat();
        let needle = q.search.trim().to_lowercase();
        let matches: Vec<&EntityRow> = self
            .rows
            .iter()
            .filter(|r| match flat {
                true => {
                    q.only.map_or(true, |set| set.contains(&r.id))
                        && (needle.is_empty() || matches_search(r, &needle))
                }
                false => match q.parent {
                    Some(p) => r.parent == Some(p),
                    None => r.parent.map_or(true, |p| !self.by_id.contains_key(&p)),
                },
            })
            .collect();
        Page {
            total: matches.len(),
            rows: matches.into_iter().skip(q.offset).take(q.limit).map(|r| self.view(r)).collect(),
        }
    }

    /// The chain of ancestors of `id`, outermost first, `id` excluded — what a UI expands to
    /// reveal a row found by search. Bounded so a parent cycle in a corrupt snapshot ends.
    pub fn ancestors(&self, id: u64) -> Vec<u64> {
        let mut chain = Vec::new();
        let mut cur = self.get(id).and_then(|r| r.parent);
        while let Some(p) = cur {
            if chain.len() > 256 || chain.contains(&p) || self.get(p).is_none() {
                break;
            }
            chain.push(p);
            cur = self.get(p).and_then(|r| r.parent);
        }
        chain.reverse();
        chain
    }
}

fn matches_search(r: &EntityRow, needle: &str) -> bool {
    if r.name.as_ref().is_some_and(|n| n.to_lowercase().contains(needle)) {
        return true;
    }
    // Digits search the id as a user sees it (index) as well as the raw bits.
    needle.chars().all(|c| c.is_ascii_digit())
        && (r.id.to_string().contains(needle) || entity_index(r.id).to_string() == needle)
}

/// The slot index of an entity's bits — the low 32 bits. The generation (high bits) is what makes a
/// recycled slot a different entity; users see the index in Bevy's own `3v1` notation.
pub fn entity_index(bits: u64) -> u32 {
    (bits & 0xFFFF_FFFF) as u32
}

/// An entity reference in any of the shapes reflection serialises one as: a bare number, a
/// one-element array (tuple struct such as `ChildOf`), `{"0": n}`, or a digit string.
pub fn parse_entity_ref(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Array(a) if a.len() == 1 => parse_entity_ref(&a[0]),
        Value::Object(o) => o.get("0").and_then(parse_entity_ref),
        _ => None,
    }
}

/// A `Name` component: a bare string, or a struct carrying a `name` field.
fn parse_name(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("name").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// The ids in a `world.query` result — the answer to "which entities have component X".
pub fn entity_ids(result: &Value) -> HashSet<u64> {
    result
        .as_array()
        .map(|a| a.iter().filter_map(|i| i.get("entity").and_then(parse_entity_ref)).collect())
        .unwrap_or_default()
}

/// A `get_components` answer (or one frame of its `+watch` stream) with the envelope removed.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ComponentsReply {
    pub components: Map<String, Value>,
    /// Components the game could not serialise: `type path → error object`.
    pub errors: Map<String, Value>,
    /// Watch frames only: components removed since the previous frame.
    pub removed: Vec<String>,
}

/// Normalise a `get_components` result. 0.15 returned the bare component map; later releases wrap it
/// in `{components, errors}` (and `+watch` frames add `removed`). A component type path always
/// contains `::`, so a literal `components` key can only be the envelope.
pub fn unwrap_components(result: &Value) -> ComponentsReply {
    let Some(obj) = result.as_object() else { return ComponentsReply::default() };
    let wrapped = obj.get("components").is_some_and(Value::is_object);
    if !wrapped {
        return ComponentsReply { components: obj.clone(), ..Default::default() };
    }
    let map_of = |k: &str| obj.get(k).and_then(Value::as_object).cloned().unwrap_or_default();
    ComponentsReply {
        components: map_of("components"),
        errors: map_of("errors"),
        removed: obj
            .get("removed")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
    }
}

/// A resource value out of `get_resources`: `{value: …}` in current releases, the bare value before.
pub fn unwrap_resource(result: &Value) -> Value {
    match result.as_object() {
        Some(o) if o.contains_key("value") => o["value"].clone(),
        _ => result.clone(),
    }
}

/// The type paths a `list_components` / `list_resources` answer names. Current releases return a
/// plain array; a few wrapped it. Anything else is an empty list.
pub fn type_paths(result: &Value) -> Vec<String> {
    let arr = result
        .as_array()
        .or_else(|| result.get("components").and_then(Value::as_array))
        .or_else(|| result.get("resources").and_then(Value::as_array));
    let mut out: Vec<String> = arr
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// The readable tail of a type path: `my_game::combat::Health` → `Health`. A generic keeps its
/// argument — `bevy_ecs::schedule::State<my_game::Phase>` → `State<Phase>` — because cutting at the
/// last `::` would leave `Phase>`.
pub fn short_type_name(type_path: &str) -> String {
    let tail = |s: &str| s.rsplit("::").next().unwrap_or(s).to_string();
    match (type_path.find('<'), type_path.ends_with('>')) {
        (Some(i), true) => format!(
            "{}<{}>",
            tail(&type_path[..i]),
            tail(&type_path[i + 1..type_path.len() - 1])
        ),
        _ => tail(type_path),
    }
}

/// Of the type paths a `registry.schema` pass classified as `Name` / `ChildOf` / `Children`, the ones
/// to actually ask for. A game may register its own `Name` next to Bevy's; asking for both works but
/// the game's would shadow the engine's in a label, so when any `bevy_*` path exists only those
/// are kept.
pub fn preferred_type_paths(found: &[String]) -> Vec<String> {
    let engine: Vec<String> = found.iter().filter(|p| p.starts_with("bevy_")).cloned().collect();
    if engine.is_empty() { found.to_vec() } else { engine }
}

/// Whether one `list_components+watch` frame says the entity's component *set* changed relative to
/// `known` (the paths currently being watched). The first frame of a watch usually restates what
/// is already known as "added", so a frame only counts when it names something new or drops
/// something watched — otherwise every watch would restart itself forever.
pub fn components_set_changed(known: &HashSet<String>, frame: &Value) -> bool {
    let list = |k: &str| -> Vec<&str> {
        frame
            .get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    };
    list("added").iter().any(|p| !known.contains(*p)) || list("removed").iter().any(|p| known.contains(*p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn short_names_keep_generic_arguments() {
        assert_eq!(short_type_name("my_game::combat::Health"), "Health");
        assert_eq!(short_type_name("bevy_ecs::schedule::State<my_game::Phase>"), "State<Phase>");
        assert_eq!(short_type_name("f32"), "f32");
    }

    #[test]
    fn engine_paths_win_over_a_games_own_name_type() {
        let found = vec!["my_game::Name".to_string(), "bevy_ecs::name::Name".to_string()];
        assert_eq!(preferred_type_paths(&found), vec!["bevy_ecs::name::Name"]);
        let only_mine = vec!["my_game::Name".to_string()];
        assert_eq!(preferred_type_paths(&only_mine), only_mine);
    }

    #[test]
    fn restated_components_are_not_a_set_change() {
        let known: HashSet<String> = ["a::A", "b::B"].iter().map(|s| s.to_string()).collect();
        assert!(!components_set_changed(&known, &json!({ "added": ["a::A", "b::B"], "removed": [] })));
        assert!(components_set_changed(&known, &json!({ "added": ["c::C"] })));
        assert!(components_set_changed(&known, &json!({ "removed": ["a::A"] })));
        assert!(!components_set_changed(&known, &json!({ "removed": ["z::Z"] })));
    }

    fn names() -> Vec<String> {
        vec!["bevy_ecs::name::Name".into()]
    }
    fn parents() -> Vec<String> {
        vec!["bevy_ecs::hierarchy::ChildOf".into()]
    }

    fn world() -> Snapshot {
        Snapshot::from_query(
            &json!([
                { "entity": 4294967296u64, "components": { "bevy_ecs::name::Name": "Root" } },
                { "entity": 4294967297u64, "components": {
                    "bevy_ecs::name::Name": "Player", "bevy_ecs::hierarchy::ChildOf": 4294967296u64 } },
                { "entity": 4294967298u64, "components": {
                    "bevy_ecs::hierarchy::ChildOf": [4294967297u64] } },
                { "entity": 4294967299u64, "components": {} }
            ]),
            &names(),
            &parents(),
        )
    }

    #[test]
    fn query_rows_become_a_tree() {
        let s = world();
        assert_eq!(s.len(), 4);
        let roots = s.page(&PageQuery { limit: 10, ..Default::default() });
        // Root and the unnamed, unparented entity.
        assert_eq!(roots.total, 2);
        assert_eq!(roots.rows[0].child_count, 1);
        let kids = s.page(&PageQuery { parent: Some(4294967296), limit: 10, ..Default::default() });
        assert_eq!(kids.rows[0].name.as_deref(), Some("Player"));
        // A one-element array is how a tuple-struct `ChildOf` can arrive.
        let grand = s.page(&PageQuery { parent: Some(4294967297), limit: 10, ..Default::default() });
        assert_eq!(grand.rows[0].id, 4294967298);
    }

    #[test]
    fn search_is_flat_and_matches_name_or_index() {
        let s = world();
        let by_name = s.page(&PageQuery { search: "play", limit: 10, ..Default::default() });
        assert_eq!(by_name.rows.len(), 1);
        // Index 2 is the third slot: Bevy would print it `2v1`.
        let by_index = s.page(&PageQuery { search: "2", limit: 10, ..Default::default() });
        assert!(by_index.rows.iter().any(|r| r.id == 4294967298));
    }

    #[test]
    fn component_filter_restricts_and_flattens() {
        let s = world();
        let only: HashSet<u64> = [4294967298u64].into_iter().collect();
        let p = s.page(&PageQuery { only: Some(&only), limit: 10, ..Default::default() });
        assert_eq!((p.total, p.rows[0].id), (1, 4294967298));
    }

    #[test]
    fn paging_slices_without_losing_the_total() {
        let s = world();
        let p = s.page(&PageQuery { search: "", only: Some(&s.rows.iter().map(|r| r.id).collect()), offset: 1, limit: 2, ..Default::default() });
        assert_eq!((p.total, p.rows.len()), (4, 2));
    }

    #[test]
    fn dangling_parent_is_still_reachable_as_a_root() {
        let s = Snapshot::build(vec![EntityRow { id: 7, name: None, parent: Some(99) }]);
        assert_eq!(s.page(&PageQuery { limit: 5, ..Default::default() }).total, 1);
    }

    #[test]
    fn revision_tracks_content() {
        let a = world();
        let b = world();
        assert_eq!(a.revision(), b.revision());
        let c = Snapshot::build(vec![EntityRow { id: 1, name: Some("x".into()), parent: None }]);
        assert_ne!(a.revision(), c.revision());
    }

    #[test]
    fn ancestors_are_outermost_first_and_cycle_safe() {
        let s = world();
        assert_eq!(s.ancestors(4294967298), vec![4294967296, 4294967297]);
        let cyc = Snapshot::build(vec![
            EntityRow { id: 1, name: None, parent: Some(2) },
            EntityRow { id: 2, name: None, parent: Some(1) },
        ]);
        assert!(cyc.ancestors(1).len() <= 2);
    }

    #[test]
    fn components_envelope_is_normalised() {
        let wrapped = unwrap_components(&json!({
            "components": { "a::B": 1 }, "errors": { "a::C": { "message": "no" } }, "removed": ["a::D"]
        }));
        assert_eq!(wrapped.components.len(), 1);
        assert_eq!(wrapped.errors.len(), 1);
        assert_eq!(wrapped.removed, vec!["a::D"]);
        let flat = unwrap_components(&json!({ "a::B": 1 }));
        assert_eq!(flat.components.len(), 1);
    }

    #[test]
    fn resource_and_type_list_envelopes() {
        assert_eq!(unwrap_resource(&json!({ "value": { "x": 1 } })), json!({ "x": 1 }));
        assert_eq!(unwrap_resource(&json!(5)), json!(5));
        assert_eq!(type_paths(&json!(["b::B", "a::A"])), vec!["a::A", "b::B"]);
        assert_eq!(type_paths(&json!({ "resources": ["a::A"] })), vec!["a::A"]);
    }
}
