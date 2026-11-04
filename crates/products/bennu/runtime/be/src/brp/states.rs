//! Bevy states: the game's modes (menu, playing, paused), found in the type registry as `State<T>` beside
//! `NextState<T>` over an enum `T`.
//!
//! Reading one is `State<T>` (the current variant); changing one is writing `NextState<T>` to `Pending(variant)`,
//! which the game applies on its next state-transition run. This file is the assistant's and MCP's door to that
//! — the inspector's own state view does the same from the frontend with the generic resource handlers — and it
//! is where "which transitions deserve a warning" lives, so the wording is the same everywhere.

use arbor_brp::prelude::{short_type_name, unwrap_resource, Op, Shape, TypeFilter, VariantShape};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{call, live};

/// A state machine as the registry lists it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StateEntry {
    /// The enum the state is over: `my_game::AppState`.
    pub enum_path: String,
    /// The `State<T>` resource.
    pub state_path: String,
    /// The `NextState<T>` resource, when the game registered it.
    pub next_path: Option<String>,
}

/// `Prefix<T>` → `T`, for the one wrapper name asked for (`State` or `NextState`).
fn inner_of<'a>(type_path: &'a str, wrapper: &str) -> Option<&'a str> {
    let at = type_path.find(&format!("{wrapper}<"))?;
    // `NextState<` must not be read as `State<`: the wrapper has to start a path segment.
    if at > 0 && !type_path[..at].ends_with("::") {
        return None;
    }
    type_path[at + wrapper.len() + 1..].strip_suffix('>')
}

/// Pair `State<T>` with `NextState<T>` resource type paths; one entry per `T`, alphabetical.
pub(crate) fn parse_state_types<S: AsRef<str>>(type_paths: &[S]) -> Vec<StateEntry> {
    let mut found: std::collections::BTreeMap<String, StateEntry> = std::collections::BTreeMap::new();
    for path in type_paths.iter().map(AsRef::as_ref) {
        if let Some(inner) = inner_of(path, "State") {
            found
                .entry(inner.to_string())
                .or_insert_with(|| StateEntry { enum_path: inner.to_string(), state_path: String::new(), next_path: None })
                .state_path = path.to_string();
        } else if let Some(inner) = inner_of(path, "NextState") {
            found
                .entry(inner.to_string())
                .or_insert_with(|| StateEntry { enum_path: inner.to_string(), state_path: String::new(), next_path: None })
                .next_path = Some(path.to_string());
        }
    }
    found.into_values().filter(|e| !e.state_path.is_empty()).collect()
}

/// The current variant name from a `State<T>` value, whatever shape reflection gave it: `"Playing"`,
/// `{"Paused": {…}}`, `["Playing"]`, `{"0": "Playing"}`.
pub(crate) fn current_state_name(value: &Value) -> Option<String> {
    let mut v = value;
    for _ in 0..3 {
        match v {
            Value::Array(a) if a.len() == 1 => v = &a[0],
            Value::Object(o) if o.len() == 1 && o.contains_key("0") => v = &o["0"],
            _ => break,
        }
    }
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) if o.len() == 1 => o.keys().next().cloned(),
        _ => None,
    }
}

/// Moving to a state with a name like this usually throws the session away: worth saying so before it is done.
pub(crate) fn is_destructive_transition(variant: &str) -> bool {
    const WORDS: [&str; 9] = ["quit", "exit", "close", "shutdown", "gameover", "mainmenu", "title", "reset", "restart"];
    let flat: String = variant.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    WORDS.iter().any(|w| flat.contains(w)) || flat.contains("unload")
}

fn variant_name(v: &VariantShape) -> &str {
    match v {
        VariantShape::Unit { name } | VariantShape::Tuple { name, .. } | VariantShape::Struct { name, .. } => name,
    }
}

// ── The wire ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StateDto {
    /// The enum's short name: `AppState`.
    pub name: String,
    pub enum_path: String,
    /// The variant the game is in now, when it could be read.
    pub current: Option<String>,
    pub variants: Vec<String>,
    /// The game registered `NextState<T>`, so the state can be changed.
    pub can_set: bool,
}

/// Every `State<T>` resource in the registry, with its current variant and the variants it can take.
async fn read_states() -> Result<Vec<(StateEntry, StateDto)>, String> {
    let live = live()?;
    let (_, page) = live.registry.search(TypeFilter::Resource, "State<", 0, 400);
    let paths: Vec<&str> = page.iter().map(|i| i.type_path.as_str()).collect();
    let mut out = Vec::new();
    for entry in parse_state_types(&paths) {
        let current = match call(Op::GetResource, Some(json!({ "resource": entry.state_path }))).await {
            Ok(raw) => current_state_name(&unwrap_resource(&raw)),
            Err(_) => None,
        };
        let variants = match live.registry.shape(&entry.enum_path) {
            Shape::Enum { variants, .. } => variants.iter().map(|v| variant_name(&v.shape).to_string()).collect(),
            _ => Vec::new(),
        };
        let dto = StateDto {
            name: short_type_name(&entry.enum_path),
            enum_path: entry.enum_path.clone(),
            current,
            variants,
            can_set: entry.next_path.is_some(),
        };
        out.push((entry, dto));
    }
    Ok(out)
}

/// The Bevy states of the running game (its modes: menu, playing, paused), the variant each is in now and the
/// variants it can be moved to. `can_set` is false for a state the game did not register `NextState` for.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_states",
    title = "Bevy states of the running game",
    safety = read,
))]
async fn bennu_brp_states(_ctx: &BennuState) -> Result<Vec<StateDto>, String> {
    Ok(read_states().await?.into_iter().map(|(_, dto)| dto).collect())
}

/// Args for [`bennu_brp_set_state`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetStateArgs {
    /// The state's enum: its short name (`AppState`) when unambiguous, or its full path.
    pub state: String,
    /// The variant to move to, one of `variants` from `bevy_game_states`.
    pub variant: String,
}

#[derive(Debug, Serialize)]
pub struct SetStateDto {
    pub state: String,
    pub from: Option<String>,
    pub to: String,
    /// The game applies the change on its next state-transition run, so reading the state straight away may
    /// still show the old variant.
    pub applied: &'static str,
}

/// Pick the entry `wanted` names: a full path exactly, else a short name that matches one. An ambiguous short
/// name is refused with the candidates — guessing which `State` was meant would move the wrong one.
pub(crate) fn resolve_state<'a>(entries: &'a [StateEntry], wanted: &str) -> Result<&'a StateEntry, String> {
    let wanted = wanted.trim();
    if let Some(e) = entries.iter().find(|e| e.enum_path == wanted) {
        return Ok(e);
    }
    let hits: Vec<&StateEntry> = entries.iter().filter(|e| short_type_name(&e.enum_path).eq_ignore_ascii_case(wanted)).collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "no state named `{wanted}`; the game has: {}",
            if entries.is_empty() { "none".to_string() } else { entries.iter().map(|e| short_type_name(&e.enum_path)).collect::<Vec<_>>().join(", ") }
        )),
        many => Err(format!("`{wanted}` is ambiguous: {}. Use the full path.", many.iter().map(|e| e.enum_path.as_str()).collect::<Vec<_>>().join(", "))),
    }
}

/// Ask the game to move a state machine to a variant by writing `NextState<T>` to `Pending(variant)`. The
/// game applies it on its next state-transition run. A transition to a state like a menu or a quit usually
/// ends the current session; that cannot be undone from here.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_set_state",
    title = "Move the running game to another state",
    safety = write,
))]
async fn bennu_brp_set_state(_ctx: &BennuState, args: SetStateArgs) -> Result<SetStateDto, String> {
    let live = live()?;
    let states = read_states().await?;
    let entries: Vec<StateEntry> = states.iter().map(|(e, _)| e.clone()).collect();
    let entry = resolve_state(&entries, &args.state)?;
    let next = entry.next_path.clone().ok_or_else(|| {
        format!("the game did not register NextState for {}, so it cannot be changed from here", short_type_name(&entry.enum_path))
    })?;
    let sample = match live.registry.shape(&entry.enum_path) {
        Shape::Enum { variants, .. } => variants.iter().find(|v| variant_name(&v.shape) == args.variant).map(|v| v.sample.clone()),
        _ => None,
    }
    .ok_or_else(|| {
        let known = states.iter().find(|(e, _)| e.enum_path == entry.enum_path).map(|(_, d)| d.variants.join(", ")).unwrap_or_default();
        format!("`{}` is not a variant of {}; it has: {known}", args.variant, short_type_name(&entry.enum_path))
    })?;
    let from = states.iter().find(|(e, _)| e.enum_path == entry.enum_path).and_then(|(_, d)| d.current.clone());
    call(Op::InsertResource, Some(json!({ "resource": next, "value": { "Pending": sample } }))).await?;
    Ok(SetStateDto { state: short_type_name(&entry.enum_path), from, to: args.variant, applied: "on the game's next state-transition run" })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = "bevy_state::state::resources::State<game::AppState>";
    const NEXT: &str = "bevy_state::state::resources::NextState<game::AppState>";

    #[test]
    fn a_state_pairs_with_its_next_state() {
        let found = parse_state_types(&[STATE, NEXT, "bevy_state::state::resources::State<game::Paused>", "game::Score"]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], StateEntry { enum_path: "game::AppState".into(), state_path: STATE.into(), next_path: Some(NEXT.into()) });
        assert_eq!(found[1].next_path, None);
    }

    #[test]
    fn a_lone_next_state_is_not_a_state_and_a_generic_enum_stays_whole() {
        assert!(parse_state_types(&["bevy_state::NextState<game::Lone>"]).is_empty());
        assert_eq!(parse_state_types(&["bevy_state::State<game::Mode<u8>>"])[0].enum_path, "game::Mode<u8>");
    }

    #[test]
    fn the_current_variant_is_read_from_every_shape() {
        assert_eq!(current_state_name(&json!("Playing")).as_deref(), Some("Playing"));
        assert_eq!(current_state_name(&json!({ "Paused": { "by": 1 } })).as_deref(), Some("Paused"));
        assert_eq!(current_state_name(&json!(["Playing"])).as_deref(), Some("Playing"));
        assert_eq!(current_state_name(&json!({ "0": "Menu" })).as_deref(), Some("Menu"));
        assert_eq!(current_state_name(&json!({ "0": { "Paused": {} } })).as_deref(), Some("Paused"));
        assert_eq!(current_state_name(&json!(3)), None);
        assert_eq!(current_state_name(&json!({ "a": 1, "b": 2 })), None);
    }

    #[test]
    fn transitions_that_end_a_session_are_recognised() {
        for v in ["Quit", "MainMenu", "main_menu", "GameOver", "ResetLevel", "Exit"] {
            assert!(is_destructive_transition(v), "{v}");
        }
        for v in ["Playing", "Paused", "Loading"] {
            assert!(!is_destructive_transition(v), "{v}");
        }
    }

    #[test]
    fn a_state_is_found_by_path_or_unambiguous_short_name() {
        let entries = parse_state_types(&[STATE, NEXT, "bevy_state::State<other::AppState>"]);
        assert_eq!(resolve_state(&entries, "game::AppState").unwrap().enum_path, "game::AppState");
        let ambiguous = resolve_state(&entries, "AppState").unwrap_err();
        assert!(ambiguous.contains("ambiguous") && ambiguous.contains("other::AppState"), "{ambiguous}");
        let unknown = resolve_state(&entries, "Nope").unwrap_err();
        assert!(unknown.contains("no state named `Nope`"), "{unknown}");
        let single = parse_state_types(&[STATE]);
        assert_eq!(resolve_state(&single, "appstate").unwrap().enum_path, "game::AppState");
    }
}
