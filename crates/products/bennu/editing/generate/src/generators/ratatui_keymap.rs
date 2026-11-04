//! A key map for ratatui: an `Action` enum, a `from_key` that turns a crossterm `KeyEvent` into one, and the
//! labels and descriptions a help line needs — written from `ctrl+s`-style key specs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::generators::rust_support::{at_caret, require_ident};
use crate::names::pascal;
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct RatatuiKeymap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionDef {
    /// The variant, PascalCase.
    pub name: String,
    /// One or more key specs separated by commas: `q, esc`, `ctrl+s`, `shift+tab`, `f5`, `G`.
    pub keys: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapOptions {
    pub enum_name: String,
    pub actions: Vec<ActionDef>,
    /// Also write `ALL`, `key_label` and `description` for a help screen.
    pub help: bool,
}

impl Default for KeymapOptions {
    fn default() -> Self {
        let a = |name: &str, keys: &str, description: &str| ActionDef { name: name.into(), keys: keys.into(), description: description.into() };
        KeymapOptions {
            enum_name: "Action".into(),
            actions: vec![
                a("Quit", "q, ctrl+c", "Quit"),
                a("Up", "k, up", "Move up"),
                a("Down", "j, down", "Move down"),
                a("Select", "enter", "Open the selection"),
                a("Search", "/", "Search"),
                a("Refresh", "ctrl+r, f5", "Reload"),
            ],
            help: true,
        }
    }
}

impl Generator for RatatuiKeymap {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "ratatui-keymap",
            title: "Generate ratatui key map",
            description: "An Action enum with from_key(KeyEvent) from key specs such as ctrl+s, plus labels and descriptions for a help line.",
            icon: "Keyboard",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["ratatui", "crossterm"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(KeymapOptions::default()).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: KeymapOptions = parse_options(options)?;
        require_ident(&o.enum_name, "Enum name")?;
        if o.actions.is_empty() {
            return Err("Add at least one action".into());
        }
        let mut seen: Vec<(Vec<String>, String)> = Vec::new();
        let mut parsed: Vec<(&ActionDef, Vec<Key>)> = Vec::new();
        for a in &o.actions {
            require_ident(&a.name, "Action")?;
            let keys = parse_keys(&a.keys).map_err(|e| format!("{}: {e}", a.name))?;
            for k in &keys {
                let id = (k.mods.clone(), k.code.clone());
                if seen.iter().any(|(m, c)| m == &id.0 && c == &id.1) {
                    return Err(format!("{}: `{}` is already bound", a.name, k.label));
                }
                seen.push((id.0, id.1));
            }
            parsed.push((a, keys));
        }
        let mut l = Lines::new();
        write(&mut l, &o, &parsed);
        Ok(at_caret(l.finish()).note("Needs `use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};`."))
    }
}

/// One parsed key spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// `CONTROL`, `ALT`, `SHIFT` — sorted.
    pub mods: Vec<String>,
    /// The `KeyCode` expression: `KeyCode::Char('q')`, `KeyCode::F(5)`.
    pub code: String,
    /// `Ctrl+S`.
    pub label: String,
    /// An upper-case letter arrives with or without SHIFT depending on the terminal.
    pub shift_optional: bool,
}

/// `"q, ctrl+c"` → two keys.
pub fn parse_keys(specs: &str) -> Result<Vec<Key>, String> {
    let keys: Result<Vec<Key>, String> = specs.split(',').map(str::trim).filter(|s| !s.is_empty()).map(parse_key).collect();
    let keys = keys?;
    if keys.is_empty() {
        return Err("give at least one key".into());
    }
    Ok(keys)
}

pub fn parse_key(spec: &str) -> Result<Key, String> {
    let parts: Vec<&str> = if spec == "+" { vec!["+"] } else { spec.split('+').map(str::trim).collect() };
    let (key, mods) = parts.split_last().ok_or("empty key")?;
    let mut flags: Vec<String> = Vec::new();
    let mut label: Vec<String> = Vec::new();
    for m in mods {
        let (flag, name) = match m.to_lowercase().as_str() {
            "ctrl" | "control" => ("CONTROL", "Ctrl"),
            "alt" => ("ALT", "Alt"),
            "shift" => ("SHIFT", "Shift"),
            other => return Err(format!("unknown modifier `{other}`")),
        };
        if !flags.iter().any(|f| f == flag) {
            flags.push(flag.to_string());
            label.push(name.to_string());
        }
    }
    flags.sort();
    let lower = key.to_lowercase();
    let (code, name) = match lower.as_str() {
        "esc" | "escape" => ("KeyCode::Esc".to_string(), "Esc".to_string()),
        "enter" | "return" => ("KeyCode::Enter".to_string(), "Enter".to_string()),
        "tab" => ("KeyCode::Tab".to_string(), "Tab".to_string()),
        "backtab" => ("KeyCode::BackTab".to_string(), "Shift+Tab".to_string()),
        "space" => ("KeyCode::Char(' ')".to_string(), "Space".to_string()),
        "backspace" => ("KeyCode::Backspace".to_string(), "Backspace".to_string()),
        "delete" | "del" => ("KeyCode::Delete".to_string(), "Delete".to_string()),
        "insert" => ("KeyCode::Insert".to_string(), "Insert".to_string()),
        "up" => ("KeyCode::Up".to_string(), "↑".to_string()),
        "down" => ("KeyCode::Down".to_string(), "↓".to_string()),
        "left" => ("KeyCode::Left".to_string(), "←".to_string()),
        "right" => ("KeyCode::Right".to_string(), "→".to_string()),
        "home" => ("KeyCode::Home".to_string(), "Home".to_string()),
        "end" => ("KeyCode::End".to_string(), "End".to_string()),
        "pageup" => ("KeyCode::PageUp".to_string(), "PgUp".to_string()),
        "pagedown" => ("KeyCode::PageDown".to_string(), "PgDn".to_string()),
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n)) => (format!("KeyCode::F({})", &f[1..]), f.to_uppercase()),
        _ if key.chars().count() == 1 => {
            let c = key.chars().next().unwrap_or(' ');
            let escaped = if c == '\'' || c == '\\' { format!("\\{c}") } else { c.to_string() };
            (format!("KeyCode::Char('{escaped}')"), c.to_string())
        }
        _ => return Err(format!("unknown key `{key}`")),
    };
    // Shift+Tab is its own code in a terminal.
    let (code, flags, label) = if code == "KeyCode::Tab" && flags == ["SHIFT"] { ("KeyCode::BackTab".to_string(), Vec::new(), vec!["Shift".to_string()]) } else { (code, flags, label) };
    let shift_optional = key.chars().count() == 1 && key.chars().all(|c| c.is_uppercase()) && flags.is_empty();
    let mut full = label;
    full.push(if key.chars().count() == 1 && key.chars().all(char::is_alphabetic) { key.to_uppercase() } else { name });
    Ok(Key { mods: flags, code, label: full.join("+"), shift_optional })
}

fn modifiers_pattern(k: &Key) -> (String, Option<String>) {
    if k.shift_optional {
        return ("KeyModifiers::NONE | KeyModifiers::SHIFT".to_string(), None);
    }
    match k.mods.len() {
        0 => ("KeyModifiers::NONE".to_string(), None),
        1 => (format!("KeyModifiers::{}", k.mods[0]), None),
        _ => ("modifiers".to_string(), Some(format!("modifiers == {}", k.mods.iter().map(|m| format!("KeyModifiers::{m}")).collect::<Vec<_>>().join(" | ")))),
    }
}

fn write(l: &mut Lines, o: &KeymapOptions, actions: &[(&ActionDef, Vec<Key>)]) {
    let name = &o.enum_name;
    l.line("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
    l.block(format!("pub enum {name}"), |l| {
        for (a, _) in actions {
            l.line(format!("{},", pascal(&a.name)));
        }
    });
    l.blank();
    l.block(format!("impl {name}"), |l| {
        l.block("pub fn from_key(key: KeyEvent) -> Option<Self>", |l| {
            l.block("match (key.code, key.modifiers)", |l| {
                for (a, keys) in actions {
                    for k in keys {
                        let (mods, guard) = modifiers_pattern(k);
                        let guard = guard.map(|g| format!(" if {g}")).unwrap_or_default();
                        l.line(format!("({}, {mods}){guard} => Some(Self::{}),", k.code, pascal(&a.name)));
                    }
                }
                l.line("_ => None,");
            });
        });
        if o.help {
            l.blank();
            l.line(format!("pub const ALL: [{name}; {}] = [", actions.len()));
            l.indent();
            for (a, _) in actions {
                l.line(format!("Self::{},", pascal(&a.name)));
            }
            l.dedent();
            l.line("];");
            l.blank();
            l.line("/// The keys as a help line shows them, e.g. `q / Ctrl+C`.");
            l.block("pub fn key_label(self) -> &'static str", |l| {
                l.block("match self", |l| {
                    for (a, keys) in actions {
                        let label = keys.iter().map(|k| k.label.clone()).collect::<Vec<_>>().join(" / ");
                        l.line(format!("Self::{} => {:?},", pascal(&a.name), label));
                    }
                });
            });
            l.blank();
            l.block("pub fn description(self) -> &'static str", |l| {
                l.block("match self", |l| {
                    for (a, _) in actions {
                        let text = if a.description.trim().is_empty() { pascal(&a.name) } else { a.description.trim().to_string() };
                        l.line(format!("Self::{} => {:?},", pascal(&a.name), text));
                    }
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    #[test]
    fn key_specs_parse_to_codes_modifiers_and_labels() {
        let k = parse_key("ctrl+c").unwrap();
        assert_eq!((k.mods, k.code.as_str(), k.label.as_str()), (vec!["CONTROL".to_string()], "KeyCode::Char('c')", "Ctrl+C"));
        let f = parse_key("F5").unwrap();
        assert_eq!((f.code.as_str(), f.label.as_str()), ("KeyCode::F(5)", "F5"));
        assert_eq!(parse_key("shift+tab").unwrap().code, "KeyCode::BackTab");
        assert_eq!(parse_key("+").unwrap().code, "KeyCode::Char('+')");
        assert_eq!(parse_key("'").unwrap().code, "KeyCode::Char('\\'')");
        assert!(parse_key("G").unwrap().shift_optional);
        assert!(parse_key("hyper+x").is_err());
        assert!(parse_key("nonsense").is_err());
    }

    #[test]
    fn several_keys_per_action() {
        assert_eq!(parse_keys("q, esc").unwrap().len(), 2);
        assert!(parse_keys(" , ").is_err());
    }

    fn text(options: serde_json::Value) -> String {
        RatatuiKeymap.generate(&options, &sample_context()).unwrap().members.unwrap().text
    }

    #[test]
    fn from_key_matches_code_and_modifiers() {
        let t = text(serde_json::json!({}));
        assert!(t.contains("(KeyCode::Char('q'), KeyModifiers::NONE) => Some(Self::Quit),"), "{t}");
        assert!(t.contains("(KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Self::Quit),"), "{t}");
        assert!(t.contains("(KeyCode::Up, KeyModifiers::NONE) => Some(Self::Up),"), "{t}");
        assert!(t.contains("(KeyCode::F(5), KeyModifiers::NONE) => Some(Self::Refresh),"), "{t}");
        assert!(t.contains("_ => None,"), "{t}");
    }

    #[test]
    fn combined_modifiers_use_a_guard_and_capitals_accept_shift() {
        let t = text(serde_json::json!({ "actions": [
            { "name": "Mark", "keys": "ctrl+alt+m" }, { "name": "Last", "keys": "G" }
        ] }));
        assert!(t.contains("(KeyCode::Char('m'), modifiers) if modifiers == KeyModifiers::ALT | KeyModifiers::CONTROL => Some(Self::Mark),"), "{t}");
        assert!(t.contains("(KeyCode::Char('G'), KeyModifiers::NONE | KeyModifiers::SHIFT) => Some(Self::Last),"), "{t}");
    }

    #[test]
    fn the_help_part_lists_every_action_with_labels() {
        let t = text(serde_json::json!({}));
        assert!(t.contains("pub const ALL: [Action; 6] = ["), "{t}");
        assert!(t.contains("Self::Quit => \"Q / Ctrl+C\","), "{t}");
        assert!(t.contains("Self::Select => \"Enter\","), "{t}");
        assert!(!text(serde_json::json!({ "help": false })).contains("key_label"));
    }

    #[test]
    fn a_key_bound_twice_or_a_bad_spec_is_refused() {
        let ctx = sample_context();
        let dup = serde_json::json!({ "actions": [{ "name": "A", "keys": "q" }, { "name": "B", "keys": "q" }] });
        assert!(RatatuiKeymap.generate(&dup, &ctx).is_err());
        let bad = serde_json::json!({ "actions": [{ "name": "A", "keys": "wat+q" }] });
        assert!(RatatuiKeymap.generate(&bad, &ctx).is_err());
    }
}
