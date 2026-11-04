//! A running Bevy game, as tools: connect over the Bevy Remote Protocol, read the world, and — behind a
//! switch that is off until the user turns it on — change it.
//!
//! Nothing here speaks BRP. The connection, the reconnect monitor, the method dialects and the registry
//! are `crate::brp` — the same ones the inspector panel uses — and every tool is a call to one of its
//! handlers; this file is the model-facing layer: specs, the approval rule, and compact capped text.
//!
//! ## Two families
//!
//! * `bevy` — connect, query, get, resources, types. Reads, no approval.
//! * `bevy_write` — mutate, insert, remove, spawn, despawn, set a resource. **Off by default**, and when
//!   on, asks once per conversation. The game is live state with no proposal and no undo, so every change
//!   is also recorded in the run's Live card.

use bennu_live::prelude::parse_local_url;
use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Map, Value};

use super::{clip, spec, ToolHost};
use crate::live::{conversation_key, live_for};

const READ: [&str; 11] = [
    "brp_connect", "brp_query", "brp_get", "brp_resources", "brp_get_resource", "brp_types", "brp_watch",
    "brp_screenshot", "brp_stats", "brp_actions", "brp_states",
];
const WRITE: [&str; 14] = [
    "brp_mutate", "brp_insert", "brp_remove", "brp_spawn", "brp_despawn", "brp_set_resource", "brp_reparent", "brp_call",
    "brp_key", "brp_type", "brp_mouse", "brp_gamepad", "brp_action", "brp_set_state",
];

/// `brp_screenshot` leaves a picture for the loop to attach, like `screenshot_page`.
pub(super) fn is_brp_picture_tool(name: &str) -> bool {
    name == "brp_screenshot"
}

/// The key under which "the user let this conversation change the game" is remembered.
const APPROVAL_HASH: &str = "bevy-write";

pub(super) fn is_brp_read(name: &str) -> bool {
    READ.contains(&name)
}

pub(super) fn is_brp_write(name: &str) -> bool {
    WRITE.contains(&name)
}

pub(super) fn is_brp_tool(name: &str) -> bool {
    is_brp_read(name) || is_brp_write(name)
}

/// The most characters of one component's value in `brp_get`; the cap for a whole answer is below.
const MAX_COMPONENT_CHARS: usize = 1_500;
const MAX_ANSWER_CHARS: usize = 14_000;
const DEFAULT_CONNECT_WAIT_S: u64 = 30;
const MAX_CONNECT_WAIT_S: u64 = 120;
const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:15702";
/// `brp_watch`: how long the entity is observed, and how often it is read.
const DEFAULT_WATCH_S: u64 = 5;
const MAX_WATCH_S: u64 = 30;
/// `brp_screenshot`: the width asked for when the model does not say; a game window at full size costs far more than it shows.
const DEFAULT_SHOT_WIDTH: u64 = 900;
/// `brp_stats`: the most diagnostics listed.
const MAX_DIAGNOSTICS: usize = 20;
const WATCH_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

const LIVE_STATE: &str = "The game is live state: there is no proposal to review and no undo.";

pub(super) fn specs() -> Vec<ToolSpec> {
    let entity = json!({"type":"integer","description":"The entity id, as brp_query / brp_get print it."});
    let type_path = json!({"type":"string","description":"A full type path, or a short name when it is unambiguous (`Health`)."});
    vec![
        spec(
            "brp_connect",
            "Connect to a running Bevy game over the Bevy Remote Protocol (the game needs RemotePlugin + RemoteHttpPlugin; the default endpoint is http://127.0.0.1:15702). Waits for the port to open, so it can be called right after starting the game with run_recipe / run_start (the first build can take minutes: raise `wait_s`). Reuses the Bennu inspector's connection: it reconnects by itself when the game restarts. Without `endpoint`, an existing connection is reused. Loopback only.",
            json!({"type":"object","properties":{
                "endpoint":{"type":"string","description":"Default http://127.0.0.1:15702."},
                "wait_s":{"type":"integer","description":"How long to wait for the game to come up. Default 30, at most 120."}
            }}),
        ),
        spec(
            "brp_query",
            "Find entities and read component values in one call: every entity that has `components` (their values are returned), `with` and not `without`. Names are full type paths or short names (`Health`). `search` filters by the entity's Name (substring) or its id. Output is capped: `limit` rows (default 20, at most 100) and long values are cut — narrow the query rather than raising the limit. Needs brp_connect first.",
            json!({"type":"object","properties":{
                "components":{"type":"array","items":{"type":"string"},"description":"Components whose values to return."},
                "with":{"type":"array","items":{"type":"string"},"description":"Also required, values not returned."},
                "without":{"type":"array","items":{"type":"string"}},
                "search":{"type":"string","description":"Case-insensitive part of the entity's Name, or an entity id."},
                "limit":{"type":"integer"}
            }}),
        ),
        spec(
            "brp_get",
            "Read the components of one entity, with values. Without `components`, every component (each value cut at 1500 characters).",
            json!({"type":"object","properties":{
                "entity":entity.clone(),
                "components":{"type":"array","items":{"type":"string"},"description":"Only these (full or short names)."}
            },"required":["entity"]}),
        ),
        spec(
            "brp_resources",
            "List the resources in the running game (singleton data: score, time, state machines). Read one with brp_get_resource.",
            json!({"type":"object","properties":{"filter":{"type":"string","description":"Case-insensitive part of the name."}}}),
        ),
        spec(
            "brp_get_resource",
            "Read one resource's current value.",
            json!({"type":"object","properties":{"type_path":type_path.clone()},"required":["type_path"]}),
        ),
        spec(
            "brp_types",
            "Search the game's type registry: components and resources it knows, by name. With `describe`, the editable shape of one type and a skeleton value — what brp_insert / brp_spawn / brp_set_resource need to be written correctly. Cheap: answered from memory, no round-trip to the game.",
            json!({"type":"object","properties":{
                "query":{"type":"string","description":"Part of a type name."},
                "kind":{"type":"string","enum":["component","resource"]},
                "limit":{"type":"integer","description":"Default 30, at most 100."},
                "describe":{"type":"string","description":"A type path: return its shape and a skeleton value instead of searching."}
            }}),
        ),
        spec(
            "brp_mutate",
            &format!("WRITE to the running game: set one field inside a component (give `entity`) or a resource (omit it), by reflect path (`.translation.x`, `.0`, `.items[2]`). {LIVE_STATE} Off by default; asks the user once per conversation; every change is listed in the run's summary. Returns the value read back."),
            json!({"type":"object","properties":{
                "entity":entity.clone(),
                "type_path":type_path.clone(),
                "path":{"type":"string","description":"Reflect path inside the value, e.g. `.current`."},
                "value":{"description":"The new value, as JSON."}
            },"required":["type_path","path","value"]}),
        ),
        spec(
            "brp_insert",
            &format!("WRITE to the running game: insert a component on an entity (replacing it whole when present), or a resource when `entity` is omitted. {LIVE_STATE} Use brp_types describe=… for the right value shape."),
            json!({"type":"object","properties":{
                "entity":entity.clone(),"type_path":type_path.clone(),"value":{"description":"The whole value, as JSON."}
            },"required":["type_path","value"]}),
        ),
        spec(
            "brp_remove",
            &format!("WRITE to the running game: remove a component from an entity, or a resource when `entity` is omitted. {LIVE_STATE}"),
            json!({"type":"object","properties":{"entity":entity.clone(),"type_path":type_path.clone()},"required":["type_path"]}),
        ),
        spec(
            "brp_spawn",
            &format!("WRITE to the running game: spawn an entity with the given components (`{{type path: value}}`) and return its id. {LIVE_STATE}"),
            json!({"type":"object","properties":{"components":{"type":"object","additionalProperties":true}},"required":["components"]}),
        ),
        spec(
            "brp_despawn",
            &format!("WRITE to the running game: despawn an entity. {LIVE_STATE}"),
            json!({"type":"object","properties":{"entity":entity.clone()},"required":["entity"]}),
        ),
        spec(
            "brp_set_resource",
            &format!("WRITE to the running game: replace a resource whole (`value`), or set one field of it (`path` + `value`). {LIVE_STATE} Returns the value read back."),
            json!({"type":"object","properties":{
                "type_path":type_path,
                "path":{"type":"string","description":"A reflect path inside the resource; omit to replace the resource whole."},
                "value":{"description":"The new value, as JSON."}
            },"required":["type_path","value"]}),
        ),
        spec(
            "brp_watch",
            "Watch one entity for a few seconds and report which component fields changed, with their first and last value — to see what a system does to an entity (it moves, its health drops) without guessing. Read-only. `seconds` default 5, at most 30. Needs brp_connect first.",
            json!({"type":"object","properties":{
                "entity":entity.clone(),
                "components":{"type":"array","items":{"type":"string"},"description":"Only these (full or short names); default every component."},
                "seconds":{"type":"integer"}
            },"required":["entity"]}),
        ),
        spec(
            "brp_reparent",
            &format!("WRITE to the running game: move an entity under another (`parent`), or detach it from its parent when `parent` is omitted. {LIVE_STATE}"),
            json!({"type":"object","properties":{"entity":entity,"parent":{"type":"integer","description":"The new parent's entity id; omit to detach."}},"required":["entity"]}),
        ),
        spec(
            "brp_call",
            &format!("WRITE to the running game: call any Remote Protocol method by name with raw JSON `params` (the game's own methods, or one the other tools do not cover). Read-only methods (`rpc.discover`, `registry.schema`, anything with `query`, `get` or `list` in its name) run without the approval; any other asks like the write tools do. {LIVE_STATE} The answer is cut at {MAX_ANSWER_CHARS} characters."),
            json!({"type":"object","properties":{
                "method":{"type":"string","description":"e.g. `world.query`, `rpc.discover`, `my_game.spawn_wave`."},
                "params":{"description":"The method's params, as JSON."}
            },"required":["method"]}),
        ),
        spec(
            "brp_screenshot",
            "Look at the running game: a screenshot of its window, returned as an image. Needs the `arbor-bevy-remote` plugin in the game (see brp_connect's hint if it is missing). Images are expensive and the conversation keeps only the latest two: take one to answer a question, not to poll; `max_width` (default 900) keeps it cheap.",
            json!({"type":"object","properties":{"max_width":{"type":"integer","description":"Shrink to at most this many pixels wide. Default 900."}}}),
        ),
        spec(
            "brp_stats",
            "Runtime statistics of the running game: frame time (average, p50/p95/p99, max), FPS, entity count and the other diagnostics it registered, and the window size. `history` adds the last N frame times. Needs the `arbor-bevy-remote` plugin.",
            json!({"type":"object","properties":{"history":{"type":"integer","description":"Also return the last N frame times in ms (at most 600)."}}}),
        ),
        spec(
            "brp_actions",
            "List the named actions the running game registered for remote clients (`restart`, `spawn_wave`, …) with their descriptions. Run one with brp_action. Needs the `arbor-bevy-remote` plugin.",
            json!({"type":"object","properties":{}}),
        ),
        spec(
            "brp_states",
            "List the Bevy states of the running game — its modes (menu, playing, paused) — with the variant each is in now and the variants it can be moved to. `can_set: false` means the game did not register NextState for it. Change one with brp_set_state.",
            json!({"type":"object","properties":{}}),
        ),
        spec(
            "brp_set_state",
            &format!("WRITE to the running game: move a Bevy state to another variant by writing NextState<T> (`state`: its short name, e.g. AppState, or full path; `variant`: one of brp_states' variants). The game applies it on its next state-transition run. A transition to a menu, a quit, a game-over or a reset usually ends the current session. {LIVE_STATE}"),
            json!({"type":"object","properties":{"state":{"type":"string"},"variant":{"type":"string"}},"required":["state","variant"]}),
        ),
        spec(
            "brp_key",
            &format!("WRITE to the running game: press a key as if on the keyboard (`key`: `a`, `Enter`, `Escape`, `F5`, `ArrowUp`, `space` …), optionally with `modifiers` (`ctrl`, `shift`, `alt`) and `action` press (default) / down / up. {LIVE_STATE} Needs the `arbor-bevy-remote` plugin."),
            json!({"type":"object","properties":{
                "key":{"type":"string"},
                "action":{"type":"string","enum":["press","down","up"]},
                "modifiers":{"type":"array","items":{"type":"string"}},
                "hold_ms":{"type":"integer","description":"How long a press is held. Default 60."}
            },"required":["key"]}),
        ),
        spec(
            "brp_type",
            &format!("WRITE to the running game: type text (1–500 characters) into whatever has the focus — a text field, a console. {LIVE_STATE} Needs the `arbor-bevy-remote` plugin."),
            json!({"type":"object","properties":{"text":{"type":"string"},"interval_ms":{"type":"integer"}},"required":["text"]}),
        ),
        spec(
            "brp_mouse",
            &format!("WRITE to the running game: move, click, double-click, drag or scroll the mouse. `x`/`y` are logical window pixels from the top-left — take a brp_screenshot first to know where things are. {LIVE_STATE} Needs the `arbor-bevy-remote` plugin."),
            json!({"type":"object","properties":{
                "action":{"type":"string","enum":["move","down","up","click","double_click","drag","wheel"]},
                "x":{"type":"number"},"y":{"type":"number"},
                "to_x":{"type":"number","description":"drag: where it ends."},"to_y":{"type":"number"},
                "button":{"type":"string","enum":["left","right","middle","back","forward"]},
                "steps":{"type":"integer"},"duration_ms":{"type":"integer"},
                "dx":{"type":"number","description":"wheel: horizontal amount."},"dy":{"type":"number","description":"wheel: vertical amount."},
                "unit":{"type":"string","enum":["line","pixel"]}
            },"required":["action"]}),
        ),
        spec(
            "brp_gamepad",
            &format!("WRITE to the running game: press a gamepad `button` (`South`/`a`, `East`/`b`, `Start`, `DPadUp`, …) or set an `axis` (`LeftStickX` …) to `value` (-1..1). A gamepad must already be connected to the game. {LIVE_STATE} Needs the `arbor-bevy-remote` plugin."),
            json!({"type":"object","properties":{
                "button":{"type":"string"},"action":{"type":"string","enum":["press","down","up"]},
                "axis":{"type":"string"},"value":{"type":"number"},"hold_ms":{"type":"integer"}
            }}),
        ),
        spec(
            "brp_action",
            &format!("WRITE to the running game: run one of its registered actions by `name` (list them with brp_actions, which also says what each does). What an action does is the game's own business and may not be undoable. {LIVE_STATE}"),
            json!({"type":"object","properties":{"name":{"type":"string"},"arg":{"description":"The action's argument, as JSON, if it takes one."}},"required":["name"]}),
        ),
    ]
}

// ── Pure helpers ─────────────────────────────────────────────────────────────────────────────────

/// `localhost:15702` and `http://127.0.0.1:15702/` both become a loopback endpoint; anything else is refused —
/// the assistant never talks to a game on another machine.
fn local_endpoint(raw: Option<&str>) -> Result<String, String> {
    let text = raw.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(DEFAULT_ENDPOINT);
    let url = if text.contains("://") { text.to_string() } else { format!("http://{text}") };
    parse_local_url(&url).map_err(|e| format!("Imber only connects to a game on this machine: {e}"))?;
    Ok(url.trim_end_matches('/').to_string())
}

/// `text` cut at `max` characters, saying how much was left out.
fn cap(text: &str, max: usize) -> String {
    let len = text.chars().count();
    if len <= max {
        return text.to_string();
    }
    format!("{}… ({} more characters)", text.chars().take(max).collect::<String>(), len - max)
}

/// Whether the component under type path `path` is the one the model named (`Health`, `my_game::Health`).
fn names_component(path: &str, wanted: &str) -> bool {
    let wanted = wanted.trim();
    path == wanted || arbor_brp::prelude::short_type_name(path).eq_ignore_ascii_case(wanted)
}

/// The rows `bennu_brp_query` returned, one per line: `#id "Name"  Health={…} Velocity={…}`.
fn query_text(answer: &Value) -> String {
    let total = answer["total"].as_u64().unwrap_or(0);
    let shown = answer["shown"].as_u64().unwrap_or(0);
    if total == 0 {
        return "no entity matches.".into();
    }
    let rows: Vec<String> = answer["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let name = r["name"].as_str().map(|n| format!(" \"{n}\"")).unwrap_or_default();
            let values: Vec<String> = r["components"].as_object().into_iter().flatten().map(|(k, v)| format!("{k}={v}")).collect();
            format!("#{}{name}  {}", r["entity"], values.join(" "))
        })
        .collect();
    let more = if total > shown { " (the rest are cut: narrow the query or raise `limit`)".to_string() } else { String::new() };
    format!("{total} entit{} match, showing {shown}{more}\n{}", if total == 1 { "y" } else { "ies" }, rows.join("\n"))
}

/// One entity as text: its name and parent, then each component (`wanted` narrows them) with its value.
fn entity_text(entity: &Value, wanted: &[String]) -> String {
    let id = &entity["id"];
    let mut out = format!("entity #{id}");
    if let Some(n) = entity["name"].as_str() {
        out.push_str(&format!(" \"{n}\""));
    }
    if !entity["parent"].is_null() {
        out.push_str(&format!(", child of #{}", entity["parent"]));
    }
    let empty = Map::new();
    let components = entity["components"].as_object().unwrap_or(&empty);
    let mut shown = 0usize;
    for (path, value) in components {
        if !wanted.is_empty() && !wanted.iter().any(|w| names_component(path, w)) {
            continue;
        }
        shown += 1;
        out.push_str(&format!("\n{} = {}", arbor_brp::prelude::short_type_name(path), cap(&value.to_string(), MAX_COMPONENT_CHARS)));
    }
    if shown == 0 {
        out.push_str(if wanted.is_empty() { "\n(no components)" } else { "\n(none of those components is on it)" });
    }
    for (path, err) in entity["errors"].as_object().unwrap_or(&empty) {
        out.push_str(&format!("\n{} could not be read: {}", arbor_brp::prelude::short_type_name(path), err["message"].as_str().unwrap_or("unknown error")));
    }
    cap(&out, MAX_ANSWER_CHARS)
}

/// Whether a raw method only reads: the discovery and registry methods and anything whose own verb is `query`,
/// `get` or `list` (`world.query`, `world.get_components`, `world.list_resources`, `bevy/get+watch`). The rule is
/// on the name's words, so a custom method that merely sounds harmless is not trusted unless it starts with one.
fn is_read_method(method: &str) -> bool {
    let method = method.trim();
    if method == "rpc.discover" || method == "registry.schema" {
        return true;
    }
    let verb = method.rsplit(['.', '/']).next().unwrap_or(method);
    let verb = verb.split('+').next().unwrap_or(verb);
    let first = verb.split('_').next().unwrap_or(verb);
    matches!(first, "query" | "get" | "list")
}

/// What changed between two reads of one entity, one line per changed field: `Health.current: 10 → 7`.
/// Components that appeared or vanished are named too; unchanged ones are left out.
fn component_changes(first: &Map<String, Value>, last: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for (path, now) in last {
        let name = arbor_brp::prelude::short_type_name(path);
        match first.get(path) {
            None => out.push(format!("{name} added = {}", cap(&now.to_string(), 200))),
            Some(before) if before != now => value_changes(&name, before, now, &mut out),
            Some(_) => {}
        }
    }
    for path in first.keys().filter(|p| !last.contains_key(*p)) {
        out.push(format!("{} removed", arbor_brp::prelude::short_type_name(path)));
    }
    out
}

/// Descend into objects and arrays so a moving `Transform` reports `.translation[1]`, not the whole struct.
fn value_changes(prefix: &str, before: &Value, now: &Value, out: &mut Vec<String>) {
    match (before, now) {
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (k, v) in b {
                if a[k] != *v {
                    value_changes(&format!("{prefix}.{k}"), &a[k], v, out);
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, v) in b.iter().enumerate() {
                if a[i] != *v {
                    value_changes(&format!("{prefix}[{i}]"), &a[i], v, out);
                }
            }
        }
        _ => out.push(format!("{prefix}: {} → {}", cap(&before.to_string(), 100), cap(&now.to_string(), 100))),
    }
}

/// The named keys of `input` that are present, as the argument object of a handler.
fn pass(input: &Value, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for k in keys {
        if let Some(v) = input.get(*k).filter(|v| !v.is_null()) {
            out.insert((*k).to_string(), v.clone());
        }
    }
    Value::Object(out)
}

/// `bennu_brp_stats`' answer as lines a model can read: frame time, then the diagnostics, then the window.
fn stats_text(s: &Value) -> String {
    let mut out = String::new();
    let f = &s["frame"];
    if f.is_null() {
        out.push_str("no frames recorded yet.");
    } else {
        let ms = |k: &str| f[k].as_f64().unwrap_or(0.0);
        out.push_str(&format!(
            "frame time over {} frames: avg {:.1} ms ({:.0} fps), min {:.1}, p50 {:.1}, p95 {:.1}, p99 {:.1}, max {:.1} ms",
            f["frames"], ms("avg_ms"), ms("fps"), ms("min_ms"), ms("p50_ms"), ms("p95_ms"), ms("p99_ms"), ms("max_ms")
        ));
    }
    for d in s["diagnostics"].as_array().into_iter().flatten().take(MAX_DIAGNOSTICS) {
        if let (Some(path), Some(v)) = (d["path"].as_str(), d["value"].as_f64()) {
            out.push_str(&format!("\n{path}: {:.2}{}", v, d["suffix"].as_str().unwrap_or("")));
        }
    }
    let w = &s["window"];
    if !w.is_null() {
        out.push_str(&format!("\nwindow {}x{} (scale {})", w["width"], w["height"], w["scale_factor"]));
    }
    if let Some(recent) = s["frame_ms_recent"].as_array().filter(|a| !a.is_empty()) {
        let list: Vec<String> = recent.iter().filter_map(Value::as_f64).map(|v| format!("{v:.1}")).collect();
        out.push_str(&format!("\nlast frames (ms): {}", list.join(" ")));
    }
    out
}

/// `bennu_brp_states`' answer as lines: `AppState: Playing (can switch to Loading, MainMenu, GameOver)`.
fn states_text(list: &Value) -> String {
    let rows: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("?");
            let current = s["current"].as_str().unwrap_or("unknown");
            let others: Vec<&str> = s["variants"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|v| *v != current).collect();
            let can = if s["can_set"].as_bool() == Some(true) { "" } else { " [read-only: no NextState registered]" };
            format!("{name}: {current}{can} — other variants: {}", if others.is_empty() { "none".to_string() } else { others.join(", ") })
        })
        .collect();
    if rows.is_empty() {
        return "the game has no Bevy states.".to_string();
    }
    format!("{} state(s):\n{}", rows.len(), rows.join("\n"))
}

fn actions_text(list: &Value) -> String {
    let rows: Vec<String> = list["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| format!("{} — {}", a["name"].as_str().unwrap_or("?"), a["description"].as_str().filter(|d| !d.is_empty()).unwrap_or("(no description)")))
        .collect();
    if rows.is_empty() {
        return "the game registered no actions.".to_string();
    }
    format!("{} action(s):\n{}", rows.len(), rows.join("\n"))
}

fn strings(input: &Value, key: &str) -> Vec<String> {
    input.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

// ── The tools ────────────────────────────────────────────────────────────────────────────────────

impl ToolHost {
    /// The question a write raises: once per conversation, remembered in memory like a recipes approval.
    pub(super) fn brp_gate(&self, tool: &str, input: &Value) -> Result<(), String> {
        let read_only_call = tool == "brp_call" && input.get("method").and_then(Value::as_str).is_some_and(is_read_method);
        if !is_brp_write(tool) || read_only_call {
            return Ok(());
        }
        let key = format!("{}#bevy-write", conversation_key(&self.session.root, &self.session.conversation_id));
        if crate::live::approvals().is_recipe_approved(&key, APPROVAL_HASH) {
            return Ok(());
        }
        let endpoint = self.brp_status().ok().and_then(|s| s["endpoint"].as_str().map(str::to_string)).unwrap_or_else(|| "the connected game".into());
        self.confirm(
            "Let Imber change the running game?",
            &format!(
                "First change: {}\nGame: {endpoint}\n\nThe game's state is live: a change takes effect at once and cannot be undone. Approving covers every change to the game in this conversation, and each is listed in the run's summary.",
                describe_write(tool, input)
            ),
            true,
        )?;
        crate::live::approvals().approve_recipes(&key, APPROVAL_HASH);
        Ok(())
    }

    pub(super) fn run_brp(&self, call_id: &str, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            "brp_connect" => self.brp_connect(input),
            "brp_query" => self.brp_query(input),
            "brp_get" => self.brp_get(input),
            "brp_resources" => self.brp_resources(input),
            "brp_get_resource" => self.brp_get_resource(input),
            "brp_types" => self.brp_types(input),
            "brp_watch" => self.brp_watch(input),
            "brp_screenshot" => self.brp_screenshot(call_id, input),
            "brp_stats" => {
                let stats = self.brp("bennu_brp_stats", pass(input, &["history"]))?;
                Ok(clip(stats_text(&stats)))
            }
            "brp_states" => {
                let list = self.call_handler("bennu_brp_states", json!({}))?;
                Ok(clip(states_text(&list)))
            }
            "brp_actions" => {
                let list = self.call_handler("bennu_brp_actions", json!({}))?;
                Ok(clip(actions_text(&list)))
            }
            write if is_brp_write(write) => self.brp_write(write, input),
            other => Err(format!("`{other}` is not a bevy tool")),
        }
    }

    /// A handler of `crate::brp`, called with its arguments struct.
    fn brp(&self, method: &str, args: Value) -> Result<Value, String> {
        self.call_handler(method, json!({ "args": args }))
    }

    fn brp_status(&self) -> Result<Value, String> {
        self.call_handler("bennu_brp_status", json!({}))
    }

    /// The full type path for a name the model wrote, checked against the game's registry.
    fn brp_resolve(&self, name: &str, kind: &str) -> Result<String, String> {
        let v = self.brp("bennu_brp_resolve", json!({ "name": name, "kind": kind }))?;
        v.as_str().map(str::to_string).ok_or_else(|| "the registry gave no type path".into())
    }

    /// True when every process this conversation started has ended: waiting longer for its port is pointless.
    fn own_processes_all_ended(&self) -> bool {
        let me = &self.session.conversation_id;
        let mine: Vec<_> = live_for(&self.session.root, &self.session.sink).manager.list().into_iter().filter(|p| p.owner.as_deref() == Some(me.as_str())).collect();
        !mine.is_empty() && mine.iter().all(|p| p.phase.is_finished())
    }

    fn brp_connect(&self, input: &Value) -> Result<String, String> {
        let asked = input.get("endpoint").and_then(Value::as_str).filter(|s| !s.trim().is_empty());
        if asked.is_none() {
            if let Ok(s) = self.brp_status() {
                if s["connected"].as_bool() == Some(true) {
                    return Ok(format!("already connected. {}", status_text(&s)));
                }
            }
        }
        let endpoint = local_endpoint(asked)?;
        let wait = input.get("wait_s").and_then(Value::as_u64).unwrap_or(DEFAULT_CONNECT_WAIT_S).min(MAX_CONNECT_WAIT_S);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
        let mut last = String::from("no answer");
        loop {
            match self.brp("bennu_brp_connect", json!({ "endpoint": endpoint, "timeout_ms": 2_000 })) {
                Ok(status) => return Ok(format!("connected to {endpoint}. {}", status_text(&status))),
                Err(e) => last = e,
            }
            if self.session.cancelled() {
                return Err("cancelled while waiting for the game".into());
            }
            if self.own_processes_all_ended() {
                return Err(format!("the process you started has ended, so nothing will answer on {endpoint}. Read its output with run_logs. Last error: {last}"));
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Err(format!(
            "no game answered on {endpoint} within {wait} s: {last}\nThe game needs `RemotePlugin` and `RemoteHttpPlugin` added to its App (default port 15702). If it is still compiling, call brp_connect again with a larger wait_s; check progress with run_logs."
        ))
    }

    fn brp_query(&self, input: &Value) -> Result<String, String> {
        let mut args = Map::new();
        for key in ["components", "with", "without", "search", "limit"] {
            if let Some(v) = input.get(key) {
                args.insert(key.into(), v.clone());
            }
        }
        Ok(clip(query_text(&self.brp("bennu_brp_query", Value::Object(args))?)))
    }

    fn brp_get(&self, input: &Value) -> Result<String, String> {
        let id = input.get("entity").and_then(Value::as_u64).ok_or("`entity` is required")?;
        let entity = self.brp("bennu_brp_entity", json!({ "id": id }))?;
        Ok(entity_text(&entity, &strings(input, "components")))
    }

    fn brp_resources(&self, input: &Value) -> Result<String, String> {
        let list = self.call_handler("bennu_brp_resources", json!({}))?;
        let needle = input.get("filter").and_then(Value::as_str).unwrap_or("").to_lowercase();
        let rows: Vec<String> = list
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| needle.is_empty() || r["type_path"].as_str().unwrap_or("").to_lowercase().contains(&needle))
            .map(|r| format!("{}  ({})", r["short_path"].as_str().unwrap_or(""), r["type_path"].as_str().unwrap_or("")))
            .collect();
        if rows.is_empty() {
            return Ok("no resource matches.".into());
        }
        Ok(clip(format!("{} resource(s):\n{}", rows.len(), rows.join("\n"))))
    }

    fn brp_get_resource(&self, input: &Value) -> Result<String, String> {
        let name = input.get("type_path").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).ok_or("`type_path` is required")?;
        let path = self.brp_resolve(name, "resource")?;
        let r = self.brp("bennu_brp_resource", json!({ "type_path": path }))?;
        Ok(format!("{path} = {}", cap(&r["value"].to_string(), MAX_ANSWER_CHARS)))
    }

    fn brp_types(&self, input: &Value) -> Result<String, String> {
        if let Some(path) = input.get("describe").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            let full = self.brp_resolve(path, input.get("kind").and_then(Value::as_str).unwrap_or("component")).unwrap_or_else(|_| path.to_string());
            let s = self.brp("bennu_brp_type_shape", json!({ "type_path": full }))?;
            return Ok(cap(&format!("{full}\nshape: {}\nskeleton value: {}", s["shape"], s["skeleton"]), MAX_ANSWER_CHARS));
        }
        let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(30).clamp(1, 100);
        let page = self.brp("bennu_brp_types", json!({ "kind": input.get("kind"), "query": input.get("query"), "limit": limit }))?;
        let rows: Vec<String> = page["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| {
                let tags: Vec<&str> = t["reflect_types"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|r| matches!(*r, "Component" | "Resource")).collect();
                format!("{}  ({})  {}", t["short_path"].as_str().unwrap_or(""), t["type_path"].as_str().unwrap_or(""), tags.join("/"))
            })
            .collect();
        Ok(clip(format!("{} type(s) match, showing {}:\n{}", page["total"], rows.len(), rows.join("\n"))))
    }

    /// A picture of the game's window, attached to the call like a page screenshot.
    fn brp_screenshot(&self, call_id: &str, input: &Value) -> Result<String, String> {
        use base64::Engine as _;
        let width = input.get("max_width").cloned().unwrap_or_else(|| json!(DEFAULT_SHOT_WIDTH));
        let shot = self.brp("bennu_brp_screenshot", json!({ "max_width": width }))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(shot["data"].as_str().unwrap_or_default())
            .map_err(|e| format!("the game sent a screenshot that is not valid base64: {e}"))?;
        let described = self.attach_png(call_id, &bytes, "the running game")?;
        Ok(format!("screenshot of the running game ({described}) is attached to this result."))
    }

    /// Read the entity every half second for `seconds`, then report what moved. Polling, not a stream: the
    /// tool thread blocks anyway, and two reads a second is nothing next to what a game does per frame.
    fn brp_watch(&self, input: &Value) -> Result<String, String> {
        let id = input.get("entity").and_then(Value::as_u64).ok_or("`entity` is required")?;
        let wanted = strings(input, "components");
        let seconds = input.get("seconds").and_then(Value::as_u64).unwrap_or(DEFAULT_WATCH_S).clamp(1, MAX_WATCH_S);
        let read = || -> Result<Map<String, Value>, String> {
            let entity = self.brp("bennu_brp_entity", json!({ "id": id }))?;
            let all = entity["components"].as_object().cloned().unwrap_or_default();
            Ok(all.into_iter().filter(|(p, _)| wanted.is_empty() || wanted.iter().any(|w| names_component(p, w))).collect())
        };
        let first = read()?;
        let (mut last, mut reads) = (first.clone(), 1u32);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        while std::time::Instant::now() < deadline && !self.session.cancelled() {
            std::thread::sleep(WATCH_EVERY);
            last = read()?;
            reads += 1;
        }
        let changes = component_changes(&first, &last);
        if changes.is_empty() {
            return Ok(format!("entity #{id}: nothing changed in {seconds} s ({reads} reads)."));
        }
        Ok(clip(format!("entity #{id}: {} change(s) in {seconds} s ({reads} reads; first → last value)\n{}", changes.len(), changes.join("\n"))))
    }

    // ── Writes ────────────────────────────────────────────────────────────────────────────────

    fn brp_write(&self, tool: &str, input: &Value) -> Result<String, String> {
        let entity = input.get("entity").and_then(Value::as_u64);
        let value = input.get("value").cloned();
        let say = match tool {
            "brp_mutate" | "brp_insert" | "brp_remove" => {
                let kind = if entity.is_some() { "component" } else { "resource" };
                let path = self.brp_resolve(str_of(input, "type_path")?, kind)?;
                let mut args = json!({ "entity": entity, "type_path": path });
                match tool {
                    "brp_mutate" => {
                        args["path"] = json!(str_of(input, "path")?);
                        args["value"] = value.ok_or("`value` is required")?;
                        self.brp("bennu_brp_mutate", args)?;
                    }
                    "brp_insert" => {
                        args["value"] = value.ok_or("`value` is required")?;
                        self.brp("bennu_brp_insert", args)?;
                    }
                    _ => {
                        self.brp("bennu_brp_remove", args)?;
                    }
                }
                let target = entity.map(|e| format!(" on entity #{e}")).unwrap_or_default();
                let field = if tool == "brp_mutate" { format!(" {}", str_of(input, "path")?) } else { String::new() };
                let verb = match tool { "brp_mutate" => "set", "brp_insert" => "inserted", _ => "removed" };
                let what = if tool == "brp_mutate" { format!("{verb}{field} of {path}{target}") } else { format!("{verb} {path}{target}") };
                format!("{what}{}", self.read_back(entity, &path, tool != "brp_remove"))
            }
            "brp_set_resource" => {
                let path = self.brp_resolve(str_of(input, "type_path")?, "resource")?;
                let value = value.ok_or("`value` is required")?;
                match input.get("path").and_then(Value::as_str).filter(|p| !p.is_empty()) {
                    Some(field) => {
                        self.brp("bennu_brp_mutate", json!({ "type_path": path, "path": field, "value": value }))?;
                        format!("set {field} of resource {path}{}", self.read_back(None, &path, true))
                    }
                    None => {
                        self.brp("bennu_brp_insert", json!({ "type_path": path, "value": value }))?;
                        format!("replaced resource {path}{}", self.read_back(None, &path, true))
                    }
                }
            }
            "brp_spawn" => {
                let components = input.get("components").and_then(Value::as_object).ok_or("`components` is required")?;
                let mut resolved = Map::new();
                for (name, v) in components {
                    resolved.insert(self.brp_resolve(name, "component")?, v.clone());
                }
                let id = self.brp("bennu_brp_spawn", json!({ "components": resolved }))?;
                format!("spawned entity #{id} with {}", resolved.keys().map(|k| arbor_brp::prelude::short_type_name(k)).collect::<Vec<_>>().join(", "))
            }
            "brp_reparent" => {
                let id = entity.ok_or("`entity` is required")?;
                let parent = input.get("parent").and_then(Value::as_u64);
                self.brp("bennu_brp_reparent", json!({ "entity": id, "parent": parent }))?;
                match parent {
                    Some(p) => format!("moved entity #{id} under #{p}"),
                    None => format!("detached entity #{id} from its parent"),
                }
            }
            "brp_call" => {
                let method = str_of(input, "method")?;
                let answer = self.brp("bennu_brp_call", json!({ "method": method, "params": input.get("params") }))?;
                let say = format!("called {method} → {}", cap(&answer.to_string(), MAX_ANSWER_CHARS));
                // A read-only method changed nothing: it is not a change to list in the run's summary.
                if is_read_method(method) {
                    return Ok(say);
                }
                say
            }
            "brp_despawn" => {
                let id = entity.ok_or("`entity` is required")?;
                self.brp("bennu_brp_despawn", json!({ "entity": id }))?;
                format!("despawned entity #{id}")
            }
            "brp_key" => {
                self.brp("bennu_brp_key", pass(input, &["key", "action", "modifiers", "hold_ms"]))?;
                let mods = strings(input, "modifiers");
                let combo = if mods.is_empty() { str_of(input, "key")?.to_string() } else { format!("{}+{}", mods.join("+"), str_of(input, "key")?) };
                format!("pressed {combo} in the game ({})", input.get("action").and_then(Value::as_str).unwrap_or("press"))
            }
            "brp_type" => {
                self.brp("bennu_brp_text", pass(input, &["text", "interval_ms"]))?;
                format!("typed {} characters in the game", str_of(input, "text")?.chars().count())
            }
            "brp_mouse" => {
                self.brp("bennu_brp_mouse", pass(input, &["action", "x", "y", "to_x", "to_y", "button", "steps", "duration_ms", "dx", "dy", "unit"]))?;
                format!("mouse {} in the game", str_of(input, "action")?)
            }
            "brp_gamepad" => {
                self.brp("bennu_brp_gamepad", pass(input, &["button", "action", "axis", "value", "hold_ms"]))?;
                "gamepad input sent to the game".to_string()
            }
            "brp_set_state" => {
                let done = self.brp("bennu_brp_set_state", pass(input, &["state", "variant"]))?;
                let from = done["from"].as_str().map(|f| format!(" (was {f})")).unwrap_or_default();
                format!("asked the game to move state {} to {}{from}; it applies it on its next state-transition run", done["state"].as_str().unwrap_or("?"), done["to"].as_str().unwrap_or("?"))
            }
            "brp_action" => {
                let name = str_of(input, "name")?;
                let answer = self.brp("bennu_brp_action", pass(input, &["name", "arg"]))?;
                format!("ran action {name} → {}", cap(&answer.to_string(), MAX_COMPONENT_CHARS))
            }
            other => return Err(format!("`{other}` is not a bevy write tool")),
        };
        self.session.live_log().bevy_change(&say);
        Ok(say)
    }

    /// ` — now: <value>` after a write, or nothing when the value cannot be read back.
    fn read_back(&self, entity: Option<u64>, type_path: &str, present: bool) -> String {
        if !present {
            return String::new();
        }
        let value = match entity {
            Some(id) => self.brp("bennu_brp_entity", json!({ "id": id })).ok().and_then(|e| e["components"].get(type_path).cloned()),
            None => self.brp("bennu_brp_resource", json!({ "type_path": type_path })).ok().map(|r| r["value"].clone()),
        };
        match value {
            Some(v) => format!(" — now {}", cap(&v.to_string(), MAX_COMPONENT_CHARS)),
            None => String::new(),
        }
    }
}

fn str_of<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| format!("`{key}` is required"))
}

/// One line for the approval prompt: what the first change is.
fn describe_write(tool: &str, input: &Value) -> String {
    let target = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("?");
    match tool {
        "brp_mutate" => format!("set {} of {}{} to {}", target("path"), target("type_path"), entity_suffix(input), cap(&input["value"].to_string(), 120)),
        "brp_insert" => format!("insert {}{}", target("type_path"), entity_suffix(input)),
        "brp_remove" => format!("remove {}{}", target("type_path"), entity_suffix(input)),
        "brp_spawn" => "spawn an entity".to_string(),
        "brp_despawn" => format!("despawn entity #{}", input["entity"]),
        "brp_reparent" => format!("reparent entity #{}", input["entity"]),
        "brp_call" => format!("call {}", target("method")),
        "brp_set_state" => {
            let variant = target("variant");
            let ends = if crate::brp::states::is_destructive_transition(variant) { " — a transition like this usually ends the current session" } else { "" };
            format!("move state {} to {variant}{ends}", target("state"))
        }
        "brp_key" => format!("press key {}", target("key")),
        "brp_type" => "type text".to_string(),
        "brp_mouse" => format!("use the mouse ({})", target("action")),
        "brp_gamepad" => "use the gamepad".to_string(),
        "brp_action" => format!("run the game's action {}", target("name")),
        _ => format!("change resource {}", target("type_path")),
    }
}

fn entity_suffix(input: &Value) -> String {
    input.get("entity").and_then(Value::as_u64).map(|e| format!(" on entity #{e}")).unwrap_or_default()
}

/// What a connection can do, in a sentence the model can plan with.
fn status_text(s: &Value) -> String {
    let can = &s["can"];
    let verbs: Vec<&str> = ["watch", "mutate", "insert", "remove", "spawn", "despawn", "mutate_resource"].into_iter().filter(|v| can[*v].as_bool() == Some(true)).collect();
    format!(
        "{} component types and {} resource types registered; the game supports: {}.",
        s["component_types"], s["resource_types"], if verbs.is_empty() { "reads only".to_string() } else { verbs.join(", ") }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_families_are_exactly_the_specs() {
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), READ.len() + WRITE.len());
        for n in &names {
            assert!(is_brp_tool(n), "{n}");
        }
        assert!(is_brp_read("brp_query") && !is_brp_write("brp_query"));
        assert!(is_brp_write("brp_despawn") && !is_brp_read("brp_despawn"));
        assert!(!is_brp_tool("http_request"));
    }

    #[test]
    fn every_write_tool_says_the_game_has_no_undo() {
        for s in specs().into_iter().filter(|s| is_brp_write(&s.name)) {
            assert!(s.description.contains("no undo"), "{}", s.name);
            assert!(s.description.starts_with("WRITE"), "{}", s.name);
        }
    }

    #[test]
    fn only_a_game_on_this_machine_is_reachable() {
        assert_eq!(local_endpoint(None).unwrap(), "http://127.0.0.1:15702");
        assert_eq!(local_endpoint(Some("localhost:15999")).unwrap(), "http://localhost:15999");
        assert_eq!(local_endpoint(Some("http://[::1]:15702/")).unwrap(), "http://[::1]:15702");
        assert!(local_endpoint(Some("http://192.168.1.20:15702")).unwrap_err().contains("this machine"));
        assert!(local_endpoint(Some("https://localhost:15702")).is_err());
    }

    #[test]
    fn a_raw_method_is_read_only_by_the_words_of_its_name() {
        for m in ["rpc.discover", "registry.schema", "world.query", "world.get_components", "world.list_resources", "bevy/get+watch", "bevy/list"] {
            assert!(is_read_method(m), "{m}");
        }
        for m in ["world.despawn_entity", "world.mutate_components", "my_game.spawn_wave", "world.insert_resources", "getaway.run", ""] {
            assert!(!is_read_method(m), "{m}");
        }
    }

    #[test]
    fn a_watch_reports_the_field_that_moved_not_the_whole_component() {
        let a = json!({"my::Health": {"current": 10, "max": 10}, "my::Transform": {"translation": [0.0, 1.0, 0.0]}, "my::Gone": 1});
        let b = json!({"my::Health": {"current": 7, "max": 10}, "my::Transform": {"translation": [0.0, 2.0, 0.0]}, "my::New": true});
        let changes = component_changes(a.as_object().unwrap(), b.as_object().unwrap());
        assert!(changes.contains(&"Health.current: 10 → 7".to_string()), "{changes:?}");
        assert!(changes.contains(&"Transform.translation[1]: 1.0 → 2.0".to_string()), "{changes:?}");
        assert!(changes.iter().any(|c| c == "New added = true") && changes.iter().any(|c| c == "Gone removed"), "{changes:?}");
        assert_eq!(changes.len(), 4);
    }

    #[test]
    fn handler_arguments_keep_only_what_was_given() {
        let input = json!({"key": "a", "action": null, "extra": 1, "modifiers": ["ctrl"]});
        assert_eq!(pass(&input, &["key", "action", "modifiers", "hold_ms"]), json!({"key": "a", "modifiers": ["ctrl"]}));
    }

    #[test]
    fn stats_read_as_lines() {
        let s = json!({
            "frame": {"frames": 100, "avg_ms": 16.6, "fps": 60.2, "min_ms": 10.0, "p50_ms": 16.0, "p95_ms": 20.0, "p99_ms": 30.0, "max_ms": 45.0},
            "diagnostics": [{"path": "entity_count", "value": 1200.0, "suffix": ""}],
            "window": {"width": 1280.0, "height": 720.0, "scale_factor": 1.0},
            "frame_ms_recent": []
        });
        let text = stats_text(&s);
        assert!(text.contains("p99 30.0") && text.contains("entity_count: 1200.00") && text.contains("window 1280.0x720.0"), "{text}");
        assert!(stats_text(&json!({"frame": null, "diagnostics": [], "window": null})).starts_with("no frames"));
    }

    #[test]
    fn actions_read_with_their_descriptions() {
        let list = json!({"actions": [{"name": "restart", "description": "Start over"}, {"name": "x", "description": ""}]});
        let text = actions_text(&list);
        assert!(text.contains("restart — Start over") && text.contains("x — (no description)"), "{text}");
        assert_eq!(actions_text(&json!({"actions": []})), "the game registered no actions.");
    }

    #[test]
    fn states_read_as_lines_and_say_when_they_are_read_only() {
        let list = json!([
            {"name": "AppState", "current": "Playing", "variants": ["Loading", "Playing", "GameOver"], "can_set": true},
            {"name": "Paused", "current": "Running", "variants": ["Running", "Paused"], "can_set": false}
        ]);
        let text = states_text(&list);
        assert!(text.contains("AppState: Playing — other variants: Loading, GameOver"), "{text}");
        assert!(text.contains("Paused: Running [read-only: no NextState registered] — other variants: Paused"), "{text}");
        assert_eq!(states_text(&json!([])), "the game has no Bevy states.");
    }

    #[test]
    fn the_approval_prompt_warns_about_a_transition_that_ends_the_session() {
        let plain = describe_write("brp_set_state", &json!({"state": "AppState", "variant": "Paused"}));
        assert_eq!(plain, "move state AppState to Paused");
        let ends = describe_write("brp_set_state", &json!({"state": "AppState", "variant": "MainMenu"}));
        assert!(ends.contains("usually ends the current session"), "{ends}");
    }

    #[test]
    fn only_the_screenshot_leaves_a_picture() {
        assert!(is_brp_picture_tool("brp_screenshot") && !is_brp_picture_tool("brp_stats"));
    }

    #[test]
    fn long_text_is_cut_and_says_how_much() {
        assert_eq!(cap("short", 10), "short");
        assert!(cap(&"x".repeat(50), 10).ends_with("(40 more characters)"));
    }

    #[test]
    fn a_component_is_found_by_full_path_or_short_name() {
        assert!(names_component("my_game::combat::Health", "Health"));
        assert!(names_component("my_game::combat::Health", "health"));
        assert!(names_component("my_game::combat::Health", "my_game::combat::Health"));
        assert!(!names_component("my_game::combat::Health", "Mana"));
    }

    #[test]
    fn a_query_answer_reads_one_row_per_entity() {
        let answer = json!({"total": 3, "shown": 2, "rows": [
            {"entity": 4, "name": "Goblin", "components": {"Health": {"current": 5}}},
            {"entity": 9, "components": {"Health": {"current": 50}}},
        ]});
        let text = query_text(&answer);
        assert!(text.starts_with("3 entities match, showing 2"), "{text}");
        assert!(text.contains("#4 \"Goblin\"  Health={\"current\":5}") && text.contains("#9  Health="), "{text}");
        assert!(text.contains("narrow the query"));
        assert_eq!(query_text(&json!({"total": 0, "shown": 0, "rows": []})), "no entity matches.");
    }

    #[test]
    fn an_entity_shows_the_components_asked_for_and_the_ones_the_game_could_not_read() {
        let e = json!({"id": 7, "name": "Hero", "parent": 2,
            "components": {"my_game::Health": {"current": 3}, "my_game::Mana": 10},
            "errors": {"my_game::Opaque": {"message": "not serializable"}}});
        let all = entity_text(&e, &[]);
        assert!(all.contains("entity #7 \"Hero\", child of #2") && all.contains("Health = ") && all.contains("Mana = 10"), "{all}");
        assert!(all.contains("Opaque could not be read: not serializable"));
        let only = entity_text(&e, &["health".into()]);
        assert!(only.contains("Health") && !only.contains("Mana"), "{only}");
        assert!(entity_text(&e, &["Nothing".into()]).contains("none of those components"));
    }

    #[test]
    fn the_approval_prompt_names_the_first_change() {
        let m = json!({"entity": 4, "type_path": "Health", "path": ".current", "value": 0});
        assert_eq!(describe_write("brp_mutate", &m), "set .current of Health on entity #4 to 0");
        assert_eq!(describe_write("brp_despawn", &json!({"entity": 4})), "despawn entity #4");
    }
}
