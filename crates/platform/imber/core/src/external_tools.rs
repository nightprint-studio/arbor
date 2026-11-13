//! External tools: programs on this machine the user lets Imber run — a linter, a converter, a CLI of their own.
//!
//! Each tool is declared by the user in settings (the `[assistant]` table of the profile, never in a repository:
//! a program's path belongs to the machine, and a permission written in a repo is a permission that gets
//! committed). The model sees the list and calls one by name with arguments and, where allowed, environment
//! variables. It never writes a command line: the program runs **without a shell**, so `;`, `|`, `$(…)` and
//! globs reach it as the characters they are.
//!
//! What a call may do is decided here, from the declaration alone — which arguments, which variables, whether to
//! ask — so the host only spawns what [`plan`] returns. Pure: no filesystem, no process.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The longest a foreground call may run, whatever the declaration says.
pub const MAX_TIMEOUT_S: u32 = 3600;
/// A declaration's time limit when it says nothing.
pub const DEFAULT_TIMEOUT_S: u32 = 120;
/// The most characters of a call's output that reach the model; the middle of a longer one is cut.
pub const MAX_OUTPUT_CHARS: usize = 24_000;

/// When a call asks the user first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolApproval {
    /// Every call.
    EveryCall,
    /// The first call of a conversation; a yes covers the rest of it until the declaration changes.
    #[default]
    Conversation,
    /// Never: the user trusts this tool.
    Never,
}

/// One program the user lets Imber run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalTool {
    /// What the model calls it: lowercase letters, digits, `-` and `_`, starting with a letter.
    pub name: String,
    /// What it does and when to use it — the model reads this to decide.
    pub description: String,
    /// The program, as an absolute path.
    pub program: String,
    /// Arguments always passed first.
    pub args: Vec<String>,
    /// The model may add arguments after the fixed ones.
    pub extra_args: bool,
    /// Environment variables always set (over the inherited environment).
    pub env: BTreeMap<String, String>,
    /// Variables the model may set; any other is refused.
    pub env_keys: Vec<String>,
    /// It changes things (files, services, data) rather than only reading: in manual mode every call asks.
    pub writes: bool,
    pub approval: ToolApproval,
    /// Seconds a call may run before it is stopped with its whole process tree.
    pub timeout_s: u32,
    /// Off keeps the declaration without offering the tool.
    pub enabled: bool,
}

impl Default for ExternalTool {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            program: String::new(),
            args: Vec::new(),
            extra_args: true,
            env: BTreeMap::new(),
            env_keys: Vec::new(),
            writes: false,
            approval: ToolApproval::Conversation,
            timeout_s: DEFAULT_TIMEOUT_S,
            enabled: true,
        }
    }
}

/// A tool's name, or an environment variable's: ASCII, starting with a letter, at most 40 characters.
fn valid_name(name: &str, extra: &[char]) -> bool {
    let mut chars = name.chars();
    name.len() <= 40
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

fn valid_tool_name(name: &str) -> bool {
    valid_name(name, &['-', '_']) && !name.chars().any(|c| c.is_ascii_uppercase())
}

fn valid_env_key(key: &str) -> bool {
    valid_name(key, &['_'])
}

/// What is wrong with a declaration, in words for the settings page; empty when it can be offered.
pub fn problems(tool: &ExternalTool) -> Vec<String> {
    let mut out = Vec::new();
    if !valid_tool_name(&tool.name) {
        out.push("The name must be lowercase letters, digits, - or _, starting with a letter (at most 40).".to_string());
    }
    if tool.description.trim().is_empty() {
        out.push("Say what it does: the description is what Imber reads to decide when to use it.".to_string());
    }
    if tool.program.trim().is_empty() {
        out.push("Choose the program.".to_string());
    } else if !Path::new(tool.program.trim()).is_absolute() {
        out.push("The program must be an absolute path.".to_string());
    }
    if let Some(bad) = tool.env.keys().chain(tool.env_keys.iter()).find(|k| !valid_env_key(k)) {
        out.push(format!("`{bad}` is not a variable name: letters, digits and _, starting with a letter."));
    }
    if tool.timeout_s == 0 || tool.timeout_s > MAX_TIMEOUT_S {
        out.push(format!("The time limit must be between 1 and {MAX_TIMEOUT_S} seconds."));
    }
    out
}

/// The tools a run is offered: switched on, valid, the first of any two with the same name.
pub fn usable(tools: &[ExternalTool]) -> Vec<&ExternalTool> {
    let mut out: Vec<&ExternalTool> = Vec::new();
    for t in tools.iter().filter(|t| t.enabled && problems(t).is_empty()) {
        if !out.iter().any(|o| o.name == t.name) {
            out.push(t);
        }
    }
    out
}

/// The usable tool called `name`, or the sentence the model reads when there is none.
pub fn find<'a>(tools: &'a [ExternalTool], name: &str) -> Result<&'a ExternalTool, String> {
    let offered = usable(tools);
    offered.iter().copied().find(|t| t.name == name).ok_or_else(|| {
        let names: Vec<&str> = offered.iter().map(|t| t.name.as_str()).collect();
        if names.is_empty() {
            "the user has not set up any external tool".to_string()
        } else {
            format!("`{name}` is not one of the external tools: {}", names.join(", "))
        }
    })
}

/// What one call runs: the program and every argument, and the variables set over the inherited environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocation {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Check a call against its declaration and build what runs. `Err` is the sentence the model reads.
pub fn plan(tool: &ExternalTool, args: &[String], env: &BTreeMap<String, String>) -> Result<ToolInvocation, String> {
    if !args.is_empty() && !tool.extra_args {
        return Err(format!("`{}` takes no arguments of yours: it always runs as `{}`", tool.name, display(&fixed_argv(tool))));
    }
    if args.iter().chain(env.values()).any(|a| a.contains('\0')) {
        return Err("an argument or a value contains a NUL character".into());
    }
    if let Some(key) = env.keys().find(|k| !tool.env_keys.iter().any(|a| a == *k)) {
        return Err(if tool.env_keys.is_empty() {
            format!("`{}` lets you set no environment variable (`{key}` was refused)", tool.name)
        } else {
            format!("`{key}` is not a variable you may set for `{}`: only {}", tool.name, tool.env_keys.join(", "))
        });
    }
    let mut argv = fixed_argv(tool);
    argv.extend(args.iter().cloned());
    let mut vars: BTreeMap<String, String> = tool.env.clone();
    vars.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok(ToolInvocation { argv, env: vars.into_iter().collect() })
}

fn fixed_argv(tool: &ExternalTool) -> Vec<String> {
    std::iter::once(tool.program.trim().to_string()).chain(tool.args.iter().cloned()).collect()
}

/// An argument vector as a reader would type it: words with spaces or quotes are quoted. For showing only —
/// nothing runs this text.
pub fn display(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if !a.is_empty() && !a.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\')) {
                a.clone()
            } else {
                format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether this call asks the user, given whether the conversation already said yes to this declaration.
pub fn must_ask(tool: &ExternalTool, approved_in_conversation: bool) -> bool {
    match tool.approval {
        ToolApproval::EveryCall => true,
        ToolApproval::Conversation => !approved_in_conversation,
        ToolApproval::Never => false,
    }
}

/// A short fingerprint of the declaration: an approval is for this one, so editing the tool asks again.
pub fn fingerprint(tool: &ExternalTool) -> String {
    // FNV-1a over the serialized declaration: stable across runs, and no hashing crate for one line.
    let text = serde_json::to_string(tool).unwrap_or_default();
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{hash:016x}")
}

/// The list the model reads in `run_tool`'s description: one line per tool, with what it may pass.
pub fn catalogue(tools: &[&ExternalTool]) -> String {
    tools
        .iter()
        .map(|t| {
            let mut line = format!("- `{}`: {}", t.name, t.description.trim().replace('\n', " "));
            let mut notes = vec![format!("runs `{}`", display(&fixed_argv(t)))];
            notes.push(if t.extra_args { "your arguments go after".to_string() } else { "takes no arguments".to_string() });
            if !t.env_keys.is_empty() {
                notes.push(format!("env you may set: {}", t.env_keys.join(", ")));
            }
            if t.writes {
                notes.push("changes things".to_string());
            }
            line.push_str(&format!(" ({})", notes.join("; ")));
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` cut to at most about `max` characters, keeping its start and its end — where a tool says what it did
/// and how it ended — and saying how much of the middle went.
pub fn clip_output(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let head = max * 2 / 5;
    let tail = max - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}\n[… {} characters cut from the middle …]\n{end}", count - head - tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> ExternalTool {
        ExternalTool {
            name: "lint".into(),
            description: "Lints a file.".into(),
            program: if cfg!(windows) { r"C:\tools\lint.exe".into() } else { "/usr/local/bin/lint".into() },
            args: vec!["--quiet".into()],
            env: BTreeMap::from([("MODE".to_string(), "ci".to_string())]),
            env_keys: vec!["LEVEL".into()],
            ..Default::default()
        }
    }

    #[test]
    fn a_complete_declaration_has_no_problems_and_a_broken_one_says_what() {
        assert!(problems(&tool()).is_empty());
        let bad = ExternalTool { name: "Lint!".into(), program: "lint".into(), timeout_s: 0, ..tool() };
        let p = problems(&bad);
        assert_eq!(p.len(), 3, "{p:?}");
        assert!(p[1].contains("absolute"));
    }

    #[test]
    fn only_enabled_valid_tools_are_offered_and_a_repeated_name_once() {
        let off = ExternalTool { enabled: false, name: "off".into(), ..tool() };
        let twin = ExternalTool { description: "second".into(), ..tool() };
        let all = vec![tool(), off, twin, ExternalTool::default()];
        let offered = usable(&all);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].description, "Lints a file.");
        assert!(find(&all, "off").unwrap_err().contains("lint"));
    }

    #[test]
    fn a_call_appends_its_arguments_and_sets_only_allowed_variables() {
        let env = BTreeMap::from([("LEVEL".to_string(), "3".to_string())]);
        let p = plan(&tool(), &["src/a.rs".into()], &env).unwrap();
        assert_eq!(&p.argv[1..], ["--quiet", "src/a.rs"]);
        assert_eq!(p.env, vec![("LEVEL".into(), "3".into()), ("MODE".into(), "ci".into())]);

        let refused = plan(&tool(), &[], &BTreeMap::from([("PATH".to_string(), "/tmp".to_string())])).unwrap_err();
        assert!(refused.contains("only LEVEL"), "{refused}");
    }

    #[test]
    fn a_tool_without_extra_arguments_refuses_them() {
        let fixed = ExternalTool { extra_args: false, ..tool() };
        assert!(plan(&fixed, &["x".into()], &BTreeMap::new()).unwrap_err().contains("takes no arguments"));
        assert!(plan(&fixed, &[], &BTreeMap::new()).is_ok());
    }

    #[test]
    fn approval_follows_the_declaration() {
        assert!(must_ask(&tool(), false));
        assert!(!must_ask(&tool(), true));
        assert!(must_ask(&ExternalTool { approval: ToolApproval::EveryCall, ..tool() }, true));
        assert!(!must_ask(&ExternalTool { approval: ToolApproval::Never, ..tool() }, false));
    }

    #[test]
    fn editing_a_declaration_changes_its_fingerprint() {
        let a = fingerprint(&tool());
        assert_eq!(a, fingerprint(&tool()));
        assert_ne!(a, fingerprint(&ExternalTool { args: vec![], ..tool() }));
    }

    #[test]
    fn display_quotes_only_what_needs_it() {
        assert_eq!(display(&["a".into(), "two words".into(), "q\"x".into()]), "a \"two words\" \"q\\\"x\"");
    }

    #[test]
    fn a_long_output_keeps_its_start_and_its_end() {
        let text = format!("{}{}{}", "a".repeat(50), "m".repeat(1000), "z".repeat(50));
        let cut = clip_output(&text, 100);
        assert!(cut.starts_with(&"a".repeat(40)) && cut.ends_with(&"z".repeat(50)), "{cut}");
        assert!(cut.contains("1000 characters cut"));
        assert_eq!(clip_output("short", 100), "short");
    }
}
