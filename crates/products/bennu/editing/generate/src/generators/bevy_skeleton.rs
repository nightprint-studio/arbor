//! Bevy ECS skeletons written at the caret of a Rust file: a component, resource, event, state, a system
//! with its queries and resources, or a plugin that registers them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::names::{is_identifier, pascal, snake};
use crate::output::{parse_options, MemberTarget, Members, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct BevySkeleton;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Field {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BevyOptions {
    /// `component` | `resource` | `event` | `state` | `system` | `plugin`
    pub kind: String,
    /// Type name (PascalCase) for data kinds, function name for a system, plugin name for a plugin.
    pub name: String,
    /// Struct fields; for a state, the variants (type ignored).
    pub fields: Vec<Field>,
    /// `system`: components queried, as written — `&Transform`, `&mut Velocity`.
    pub query: Vec<String>,
    /// `system`: query filters — `Player` becomes `With<Player>`, `!Dead` becomes `Without<Dead>`.
    pub filters: Vec<String>,
    /// `system`: resources — `Time`, `mut Score`.
    pub resources: Vec<String>,
    pub commands: bool,
    /// `plugin`: systems per schedule, resources to init, events to register.
    pub update_systems: Vec<String>,
    pub startup_systems: Vec<String>,
    pub init_resources: Vec<String>,
    pub events: Vec<String>,
    /// The Bevy minor version (`16` for 0.16): 0.17 renamed buffered events to messages.
    pub bevy_minor: u32,
}

impl Default for BevyOptions {
    fn default() -> Self {
        BevyOptions {
            kind: "component".into(),
            name: String::new(),
            fields: Vec::new(),
            query: Vec::new(),
            filters: Vec::new(),
            resources: Vec::new(),
            commands: false,
            update_systems: Vec::new(),
            startup_systems: Vec::new(),
            init_resources: Vec::new(),
            events: Vec::new(),
            bevy_minor: 16,
        }
    }
}

impl Generator for BevySkeleton {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "bevy-skeleton",
            title: "Generate Bevy component, system or plugin",
            description: "ECS skeletons at the caret: components, resources, events, states, systems with queries, and plugins that register them.",
            icon: "Boxes",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["bevy"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(BevyOptions { name: "Health".into(), fields: vec![Field { name: "current".into(), ty: "f32".into() }, Field { name: "max".into(), ty: "f32".into() }], ..BevyOptions::default() })
            .unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: BevyOptions = parse_options(options)?;
        if !is_identifier(&o.name) {
            return Err(format!("`{}` is not a valid name", o.name));
        }
        let mut l = Lines::new();
        let mut notes: Vec<String> = Vec::new();
        match o.kind.as_str() {
            "component" => data_type(&mut l, &o, "Component", &["Debug", "Clone", "Default"]),
            "resource" => data_type(&mut l, &o, "Resource", &["Debug", "Default"]),
            "event" => {
                let derive = if o.bevy_minor >= 17 { "Message" } else { "Event" };
                data_type(&mut l, &o, derive, &["Debug", "Clone"]);
            }
            "state" => state(&mut l, &o)?,
            "system" => system(&mut l, &o, &mut notes),
            "plugin" => plugin(&mut l, &o),
            other => return Err(format!("Unknown kind `{other}`")),
        }
        let mut out = Output::default();
        out.members = Some(Members { target: MemberTarget::Caret, text: l.finish().trim_end().to_string(), imports: Vec::new() });
        out = out.note("Needs `use bevy::prelude::*;` in the file.");
        for n in notes {
            out = out.note(n);
        }
        Ok(out)
    }
}

fn data_type(l: &mut Lines, o: &BevyOptions, bevy_derive: &str, extra: &[&str]) {
    let mut derives = vec![bevy_derive.to_string()];
    derives.extend(extra.iter().map(|d| d.to_string()));
    if o.fields.is_empty() {
        l.line(format!("#[derive({})]", derives.join(", ")));
        l.line(format!("pub struct {};", o.name));
        return;
    }
    l.line(format!("#[derive({})]", derives.join(", ")));
    l.block(format!("pub struct {}", o.name), |l| {
        for f in &o.fields {
            l.line(format!("pub {}: {},", snake(&f.name), if f.ty.trim().is_empty() { "f32" } else { f.ty.trim() }));
        }
    });
}

fn state(l: &mut Lines, o: &BevyOptions) -> Result<(), String> {
    let variants: Vec<String> = o.fields.iter().map(|f| pascal(&f.name)).collect();
    if variants.is_empty() {
        return Err("A state needs at least one variant — list them as fields".into());
    }
    l.line("#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]");
    l.block(format!("pub enum {}", o.name), |l| {
        for (i, v) in variants.iter().enumerate() {
            if i == 0 {
                l.line("#[default]");
            }
            l.line(format!("{v},"));
        }
    });
    Ok(())
}

/// `&mut Velocity` → (`mut velocity`, `&mut Velocity`).
fn binding(component: &str) -> (String, bool) {
    let t = component.trim();
    let mutable = t.starts_with("&mut ");
    let name = t.trim_start_matches('&').trim_start_matches("mut ").trim();
    let simple = name.rsplit("::").next().unwrap_or(name);
    (if mutable { format!("mut {}", snake(simple)) } else { snake(simple) }, mutable)
}

fn system(l: &mut Lines, o: &BevyOptions, notes: &mut Vec<String>) {
    let name = snake(&o.name);
    let mut params: Vec<String> = Vec::new();
    if o.commands {
        params.push("mut commands: Commands".to_string());
    }
    for r in &o.resources {
        let r = r.trim();
        match r.strip_prefix("mut ") {
            Some(ty) => params.push(format!("mut {}: ResMut<{}>", snake(ty.trim()), ty.trim())),
            None => params.push(format!("{}: Res<{}>", snake(r), r)),
        }
    }
    let filters: Vec<String> = o
        .filters
        .iter()
        .map(|f| match f.trim().strip_prefix('!') {
            Some(ty) => format!("Without<{}>", ty.trim()),
            None => format!("With<{}>", f.trim()),
        })
        .collect();
    let bindings: Vec<(String, bool)> = o.query.iter().map(|c| binding(c)).collect();
    let any_mut = bindings.iter().any(|b| b.1);
    if !o.query.is_empty() {
        let data = if o.query.len() == 1 { o.query[0].trim().to_string() } else { format!("({})", o.query.iter().map(|c| c.trim()).collect::<Vec<_>>().join(", ")) };
        let filter = match filters.len() {
            0 => String::new(),
            1 => format!(", {}", filters[0]),
            _ => format!(", ({})", filters.join(", ")),
        };
        params.push(format!("{}query: Query<{data}{filter}>", if any_mut { "mut " } else { "" }));
    }
    l.block(format!("pub fn {name}({})", params.join(", ")), |l| {
        if o.query.is_empty() {
            l.line("// TODO");
            return;
        }
        let pattern = if bindings.len() == 1 { bindings[0].0.clone() } else { format!("({})", bindings.iter().map(|b| b.0.as_str()).collect::<Vec<_>>().join(", ")) };
        l.block(format!("for {pattern} in {}query", if any_mut { "&mut " } else { "&" }), |l| {
            l.line("// TODO");
        });
    });
    if params.is_empty() {
        notes.push("A system without parameters does nothing — add a query or a resource.".to_string());
    }
}

fn plugin(l: &mut Lines, o: &BevyOptions) {
    let name = if o.name.ends_with("Plugin") { o.name.clone() } else { format!("{}Plugin", o.name) };
    l.line(format!("pub struct {name};"));
    l.blank();
    l.block(format!("impl Plugin for {name}"), |l| {
        l.block("fn build(&self, app: &mut App)", |l| {
            let mut calls: Vec<String> = Vec::new();
            for r in &o.init_resources {
                calls.push(format!(".init_resource::<{}>()", r.trim()));
            }
            for e in &o.events {
                calls.push(if o.bevy_minor >= 17 { format!(".add_message::<{}>()", e.trim()) } else { format!(".add_event::<{}>()", e.trim()) });
            }
            if !o.startup_systems.is_empty() {
                calls.push(format!(".add_systems(Startup, {})", system_set(&o.startup_systems)));
            }
            if !o.update_systems.is_empty() {
                calls.push(format!(".add_systems(Update, {})", system_set(&o.update_systems)));
            }
            if calls.is_empty() {
                l.line("// TODO: register resources, events and systems");
                return;
            }
            let mut chain = vec![format!("app{}", calls[0])];
            chain.extend(calls.iter().skip(1).cloned());
            if let Some(last) = chain.last_mut() {
                last.push(';');
            }
            l.line(&chain[0]);
            l.indent().indent();
            for line in &chain[1..] {
                l.line(line);
            }
            l.dedent().dedent();
        });
    });
}

fn system_set(systems: &[String]) -> String {
    let names: Vec<String> = systems.iter().map(|s| snake(s.trim())).collect();
    if names.len() == 1 { names[0].clone() } else { format!("({})", names.join(", ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn text(options: serde_json::Value) -> String {
        BevySkeleton.generate(&options, &sample_context()).expect("generates").members.unwrap().text
    }

    #[test]
    fn a_component_derives_and_snake_cases_its_fields() {
        let t = text(serde_json::json!({ "kind": "component", "name": "Health", "fields": [{ "name": "maxHp", "ty": "f32" }] }));
        assert_eq!(t, "#[derive(Component, Debug, Clone, Default)]\npub struct Health {\n    pub max_hp: f32,\n}");
    }

    #[test]
    fn a_marker_component_is_a_unit_struct() {
        let t = text(serde_json::json!({ "kind": "component", "name": "Player" }));
        assert_eq!(t, "#[derive(Component, Debug, Clone, Default)]\npub struct Player;");
    }

    #[test]
    fn events_become_messages_from_bevy_0_17() {
        assert!(text(serde_json::json!({ "kind": "event", "name": "Hit", "bevy_minor": 16 })).contains("derive(Event"));
        assert!(text(serde_json::json!({ "kind": "event", "name": "Hit", "bevy_minor": 17 })).contains("derive(Message"));
    }

    #[test]
    fn a_state_marks_the_first_variant_default() {
        let t = text(serde_json::json!({ "kind": "state", "name": "GameState", "fields": [{ "name": "loading" }, { "name": "playing" }] }));
        assert!(t.contains("pub enum GameState {\n    #[default]\n    Loading,\n    Playing,\n}"), "{t}");
        assert!(BevySkeleton.generate(&serde_json::json!({ "kind": "state", "name": "S" }), &sample_context()).is_err());
    }

    #[test]
    fn a_system_takes_resources_and_a_filtered_mutable_query() {
        let t = text(serde_json::json!({
            "kind": "system", "name": "MovePlayer",
            "resources": ["Time", "mut Score"],
            "query": ["&mut Transform", "&Speed"],
            "filters": ["Player", "!Dead"]
        }));
        assert!(
            t.contains("pub fn move_player(time: Res<Time>, mut score: ResMut<Score>, mut query: Query<(&mut Transform, &Speed), (With<Player>, Without<Dead>)>) {"),
            "{t}"
        );
        assert!(t.contains("for (mut transform, speed) in &mut query {"), "{t}");
    }

    #[test]
    fn a_single_read_only_component_iterates_by_shared_reference() {
        let t = text(serde_json::json!({ "kind": "system", "name": "report", "query": ["&Health"] }));
        assert!(t.contains("pub fn report(query: Query<&Health>) {"), "{t}");
        assert!(t.contains("for health in &query {"), "{t}");
    }

    #[test]
    fn a_plugin_chains_its_registrations() {
        let t = text(serde_json::json!({
            "kind": "plugin", "name": "Combat",
            "init_resources": ["Score"], "events": ["Hit"],
            "startup_systems": ["setup"], "update_systems": ["move_player", "apply_damage"]
        }));
        assert!(t.contains("pub struct CombatPlugin;"), "{t}");
        assert!(t.contains("app.init_resource::<Score>()"), "{t}");
        assert!(t.contains(".add_event::<Hit>()"), "{t}");
        assert!(t.contains(".add_systems(Startup, setup)"), "{t}");
        assert!(t.contains(".add_systems(Update, (move_player, apply_damage));"), "{t}");
    }

    #[test]
    fn a_single_registration_ends_with_a_semicolon() {
        let t = text(serde_json::json!({ "kind": "plugin", "name": "Solo", "init_resources": ["Score"] }));
        assert!(t.contains("app.init_resource::<Score>();"), "{t}");
    }
}
