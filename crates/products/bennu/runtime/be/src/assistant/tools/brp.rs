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

const READ: [&str; 6] = ["brp_connect", "brp_query", "brp_get", "brp_resources", "brp_get_resource", "brp_types"];
const WRITE: [&str; 6] = ["brp_mutate", "brp_insert", "brp_remove", "brp_spawn", "brp_despawn", "brp_set_resource"];

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
            json!({"type":"object","properties":{"entity":entity},"required":["entity"]}),
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

fn strings(input: &Value, key: &str) -> Vec<String> {
    input.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

// ── The tools ────────────────────────────────────────────────────────────────────────────────────

impl ToolHost {
    /// The question a write raises: once per conversation, remembered in memory like a recipes approval.
    pub(super) fn brp_gate(&self, tool: &str, input: &Value) -> Result<(), String> {
        if !is_brp_write(tool) {
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

    pub(super) fn run_brp(&self, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            "brp_connect" => self.brp_connect(input),
            "brp_query" => self.brp_query(input),
            "brp_get" => self.brp_get(input),
            "brp_resources" => self.brp_resources(input),
            "brp_get_resource" => self.brp_get_resource(input),
            "brp_types" => self.brp_types(input),
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
            "brp_despawn" => {
                let id = entity.ok_or("`entity` is required")?;
                self.brp("bennu_brp_despawn", json!({ "entity": id }))?;
                format!("despawned entity #{id}")
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
