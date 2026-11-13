//! `run_script`: a script of the project that is not one of the user's programs, run only after the user has
//! read it.
//!
//! The card shows the script — its path, the command, its first lines — and three answers: **no**, **run it
//! once**, or **run it and add it to my programs** (the panel saves a declaration for `run_tool`, filled in from
//! the script, which the user can refine in Settings). A "once" covers this call; an "add" covers the script for
//! the rest of the conversation — **as it is now**: Imber can edit the project's files, so a yes is bound to the
//! script's content, and a script that changed asks again, saying so.
//!
//! How it runs is decided by the file, never by a command line: an executable file directly, otherwise its
//! interpreter by extension (`sh`, `bash`, `python3`, `node`, PowerShell, `cmd` on Windows), with the arguments
//! passed one by one. The run itself is `run_tool`'s: output, exit code, a time limit, the whole process tree
//! stopped, or a managed process with `background`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bennu_assistant::prelude::{display_argv, ExternalTool, ToolApproval, ToolInvocation, ToolSpec};
use serde_json::{json, Value};

use super::{spec, unique_id, ToolHost};
use crate::live::{approvals, conversation_key};

pub(super) const NAME: &str = "run_script";

/// How long a script may run in the foreground.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(600);
/// Lines of the script the card shows.
const PREVIEW_LINES: usize = 40;
/// Characters of the script the card shows.
const PREVIEW_CHARS: usize = 4_000;

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        NAME,
        "Run a script of the project (.sh, .bash, .py, .js, .ps1, .bat/.cmd, or an executable file) that is not one of \
         the user's programs (those are run with run_tool). The user is shown the script and decides: decline, run it \
         once, or run it and add it to their programs — after which run_tool runs it. A yes is for the script as it is: \
         if you edit it, the user is asked again. Arguments are passed one by one, with no shell. It runs in `cwd` \
         (inside the project, default its root) and returns the exit code with stdout and stderr, stopped after 10 \
         minutes; with `background: true` it starts as a managed process (run_logs, run_stop).",
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"The script, relative to the project (or `project:path`)."},
            "args":{"type":"array","items":{"type":"string"},"description":"Its arguments, one per item."},
            "cwd":{"type":"string","description":"Folder relative to the project. Default: the project root."},
            "stdin":{"type":"string","description":"Text sent to its standard input (foreground only)."},
            "background":{"type":"boolean","description":"Start it as a managed process and return at once."},
            "name":{"type":"string","description":"With background: the process's name. Default: the script's name."},
            "why":{"type":"string","description":"One sentence for the user: what running it does and why now."}
        },"required":["path"]}),
    )]
}

pub(super) fn is_script_tool(name: &str) -> bool {
    name == NAME
}

/// The program and the leading arguments that run `script`: the file itself when it is executable, otherwise its
/// interpreter by extension.
fn launcher(script: &Path) -> Result<Vec<String>, String> {
    let file = script.display().to_string();
    let ext = script.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let find = |names: &[&str]| -> Option<String> {
        names.iter().find_map(|n| arbor_process_ext::prelude::locate_executable(n, None, &[], &[]))
    };
    let with = |program: Option<String>, what: &str, lead: &[&str]| -> Result<Vec<String>, String> {
        let program = program.ok_or_else(|| format!("{what} was not found on this machine, so `{file}` cannot be run"))?;
        Ok(std::iter::once(program).chain(lead.iter().map(|s| s.to_string())).chain(std::iter::once(file.clone())).collect())
    };
    match ext.as_str() {
        "sh" => with(if cfg!(windows) { find(&["sh"]) } else { Some("/bin/sh".into()) }, "sh", &[]),
        "bash" => with(find(&["bash"]), "bash", &[]),
        "py" => with(find(&["python3", "python"]), "Python", &[]),
        "js" | "mjs" | "cjs" => with(find(&["node"]), "Node", &[]),
        "ps1" => with(find(&["pwsh", "powershell"]), "PowerShell", &["-NoProfile", "-File"]),
        "bat" | "cmd" if cfg!(windows) => with(find(&["cmd"]), "cmd", &["/C"]),
        "bat" | "cmd" => Err(format!("`{file}` is a Windows batch file; it does not run on this machine")),
        _ if is_executable(script) => Ok(vec![file]),
        _ => Err(format!(
            "`{file}` is neither an executable file nor a kind of script known here (.sh, .bash, .py, .js, .ps1, .bat, .cmd)"
        )),
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// The first lines of the script, for the card, and how many lines it has.
fn preview(text: &str) -> (String, usize) {
    let total = text.lines().count();
    let mut shown: String = text.lines().take(PREVIEW_LINES).collect::<Vec<_>>().join("\n");
    if shown.chars().count() > PREVIEW_CHARS {
        shown = shown.chars().take(PREVIEW_CHARS).collect();
    }
    (shown, total)
}

/// What the panel saves when the user adds the script to their programs: a declaration for `run_tool`, to refine.
fn declaration(script: &Path, launch: &[String], label: &str) -> ExternalTool {
    let stem = script.file_stem().and_then(|s| s.to_str()).unwrap_or("script");
    let mut name: String = stem.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    name = name.trim_start_matches(|c: char| !c.is_ascii_alphabetic()).chars().take(40).collect();
    if name.is_empty() {
        name = "script".into();
    }
    ExternalTool {
        name,
        description: format!("Runs the project's script {label}. Added from a conversation: say here what it does."),
        program: launch[0].clone(),
        args: launch[1..].to_vec(),
        extra_args: true,
        writes: true,
        approval: ToolApproval::Conversation,
        timeout_s: SCRIPT_TIMEOUT.as_secs() as u32,
        ..Default::default()
    }
}

impl ToolHost {
    fn script_path(&self, input: &Value) -> Result<PathBuf, String> {
        let raw = input.get("path").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or("`path` is required")?;
        let path = self.resolve(raw);
        if !path.is_file() {
            return Err(format!("`{}` is not a file", path.display()));
        }
        Ok(path)
    }

    /// What a call reads (the script) and where it runs, for the access policy.
    pub(super) fn script_touched(&self, input: &Value) -> Vec<(PathBuf, bennu_assistant::prelude::Op)> {
        let mut out = self.external_touched(input);
        if let Ok(p) = self.script_path(input) {
            out.push((p, bennu_assistant::prelude::Op::Read));
        }
        out
    }

    fn script_key(&self, script: &Path) -> String {
        format!("{}#script:{}", conversation_key(&self.session.root, &self.session.conversation_id), script.display())
    }

    /// Show the user the script and take their answer: run it once, run it and add it, or not at all.
    pub(super) fn script_gate(&self, input: &Value) -> Result<(), String> {
        let script = self.script_path(input)?;
        let launch = launcher(&script)?;
        let text = std::fs::read(&script).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| format!("`{}` could not be read: {e}", script.display()))?;
        let stamp = bennu_live::prelude::content_hash(&text);
        let key = self.script_key(&script);
        if approvals().is_recipe_approved(&key, &stamp) {
            return Ok(());
        }
        let seen_key = format!("{key}#seen");
        let changed = approvals().seen_hash(&seen_key).is_some_and(|seen| seen != stamp);
        let args: Vec<String> = input.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let label = crate::assistant::projects::labelled(&self.session.projects(), &script).unwrap_or_else(|| script.display().to_string());
        let argv: Vec<String> = launch.iter().cloned().chain(args.iter().cloned()).collect();
        let (shown, lines) = preview(&text);
        let id = unique_id("sc");
        let block = json!({
            "type": "script",
            "script": {
                "id": id,
                "path": script.display().to_string(),
                "label": label,
                "command": display_argv(&argv),
                "cwd": input.get("cwd").and_then(Value::as_str).unwrap_or("."),
                "why": input.get("why").and_then(Value::as_str).unwrap_or(""),
                "preview": shown,
                "lines": lines,
                "changed": changed,
                "background": input.get("background").and_then(Value::as_bool).unwrap_or(false),
                "declaration": declaration(&script, &launch, &label),
            },
        });
        if self.session.unattended {
            return Err("this is a scheduled run: nobody is there to read the script, so it was not run".into());
        }
        let answer = self.session.ask(block, "/script/id", &id, |b, a| {
            b["script"]["answer"] = a.get("answer").cloned().unwrap_or(json!("no"));
        });
        match answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) {
            Some("once") => {
                approvals().remember_seen(&seen_key, &stamp);
                Ok(())
            }
            Some("add") => {
                approvals().remember_seen(&seen_key, &stamp);
                approvals().approve_recipes(&key, &stamp);
                Ok(())
            }
            _ => Err("the user declined to run the script".into()),
        }
    }

    pub(super) fn run_script(&self, input: &Value) -> Result<String, String> {
        let script = self.script_path(input)?;
        let launch = launcher(&script)?;
        let args = input.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>()).unwrap_or_default();
        if args.iter().any(|a| a.contains('\0')) {
            return Err("an argument contains a NUL character".into());
        }
        let name = script.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "script".into());
        let inv = ToolInvocation { argv: launch.into_iter().chain(args).collect(), env: Vec::new() };
        self.run_program(&name, inv, input, SCRIPT_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_script_runs_through_sh_and_its_declaration_keeps_that() {
        if cfg!(windows) {
            return;
        }
        let launch = launcher(Path::new("/p/scripts/Install Plugin.sh")).unwrap();
        assert_eq!(launch, vec!["/bin/sh".to_string(), "/p/scripts/Install Plugin.sh".to_string()]);
        let d = declaration(Path::new("/p/scripts/Install Plugin.sh"), &launch, "scripts/Install Plugin.sh");
        assert_eq!((d.name.as_str(), d.program.as_str(), d.args.len()), ("install-plugin", "/bin/sh", 1));
        assert!(d.writes && d.approval == ToolApproval::Conversation);
    }

    #[test]
    fn an_unknown_file_is_refused_with_the_kinds_known() {
        let err = launcher(Path::new("/nowhere/notes.txt")).unwrap_err();
        assert!(err.contains(".sh") && err.contains("executable"), "{err}");
    }

    #[test]
    fn the_preview_is_the_first_lines_and_says_how_many_there_are() {
        let text = (1..=100).map(|n| format!("echo {n}")).collect::<Vec<_>>().join("\n");
        let (shown, total) = preview(&text);
        assert_eq!(total, 100);
        assert_eq!(shown.lines().count(), PREVIEW_LINES);
    }
}
