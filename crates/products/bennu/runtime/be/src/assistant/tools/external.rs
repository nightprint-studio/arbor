//! `run_tool`: the programs the user declared in Settings › Imber › External tools, run for the model.
//!
//! The declaration decides everything a call may do — see `bennu_assistant::external_tools`; this file is the
//! model-facing layer: the spec (with the list of tools in its description), the question to the user, and the
//! run itself. A call runs the program **directly**, with no shell, in a folder of the project:
//!
//! * in the foreground (the default) it waits for the program to end, up to the declaration's time limit, and
//!   returns the exit code with stdout and stderr; past the limit, or when the run is cancelled, the whole
//!   process tree is killed;
//! * with `background: true` it becomes a managed process of the Live family (`run_logs`, `run_stop`), for a
//!   watcher or a server.
//!
//! ## What asks the user
//!
//! The declaration's `approval`: every call, once per conversation (until the declaration changes), or never. A
//! tool that changes things asks at every call in manual mode, whatever it says — manual is the mode in which
//! changes are looked at first. A scheduled run is not offered `run_tool` (it is in the Run family, which a
//! scheduled run never gets), and the gate refuses it there as a process all the same.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bennu_assistant::prelude::{
    clip_output, display_argv, external_tools_catalogue, find_external_tool, plan_tool_call, tool_fingerprint, tool_must_ask, usable_tools, ExternalTool,
    ToolApproval, ToolInvocation, ToolSpec, TOOL_MAX_OUTPUT_CHARS,
};
use bennu_live::prelude::{kill_tree, own_group, Readiness, StartSpec};
use serde_json::{json, Value};

use super::live::{duration_text, readiness_from};
use super::{spec, ToolHost};
use crate::live::{approvals, conversation_key};

pub(super) const NAME: &str = "run_tool";

/// The most bytes of one stream kept while a program runs; what comes after is read and dropped, so a chatty
/// program neither fills the memory nor blocks on a full pipe.
const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
/// The most text sent to a program's standard input.
const MAX_STDIN_CHARS: usize = 1024 * 1024;

pub(super) fn is_external_tool(name: &str) -> bool {
    name == NAME
}

/// The user's declarations, as saved.
fn declared() -> Vec<ExternalTool> {
    bennu_core::config::load().assistant.external_tools
}

/// `run_tool`, when the user has at least one tool switched on: a definition with nothing to call is only noise.
pub(super) fn specs() -> Vec<ToolSpec> {
    let tools = declared();
    let offered = usable_tools(&tools);
    if offered.is_empty() {
        return Vec::new();
    }
    vec![spec(
        NAME,
        &format!(
            "Run one of the programs the user set up for you, by name. It runs directly, with NO shell: give each \
             argument as its own string (nothing is expanded, split or chained). It runs in `cwd` (inside the \
             project, default its root) and returns the exit code with stdout and stderr; it is stopped with its \
             whole process tree at its time limit. With `background: true` it is started as a managed process \
             instead (read it with run_logs, end it with run_stop) — for a watcher or a server. The user may be \
             asked to approve a call.\n\nThe tools:\n{}",
            external_tools_catalogue(&offered)
        ),
        json!({"type":"object","properties":{
            "tool":{"type":"string","description":"The tool's name, as listed."},
            "args":{"type":"array","items":{"type":"string"},"description":"Arguments after the tool's fixed ones, one per item. Only where the list says it takes yours."},
            "env":{"type":"object","additionalProperties":{"type":"string"},"description":"Environment variables, only those the list says you may set."},
            "cwd":{"type":"string","description":"Folder relative to the project. Default: the project root."},
            "stdin":{"type":"string","description":"Text sent to the program's standard input (foreground only)."},
            "background":{"type":"boolean","description":"Start it as a managed process and return at once. Default false."},
            "name":{"type":"string","description":"With background: the process's name for run_logs/run_stop. Default: the tool's name."},
            "ready":{"type":"object","description":"With background: when it is ready, as in run_start (port, log, http, status, delay_ms, timeout_s)."}
        },"required":["tool"]}),
    )]
}

/// The arguments and variables of a call, as given.
fn call_parts(input: &Value) -> (Vec<String>, BTreeMap<String, String>) {
    let args = input.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let env = input
        .get("env")
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
        .unwrap_or_default();
    (args, env)
}

/// The declaration a call names and what it would run, or why it cannot.
fn resolve(input: &Value) -> Result<(ExternalTool, ToolInvocation), String> {
    let name = input.get("tool").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or("`tool` is required")?;
    let tools = declared();
    let tool = find_external_tool(&tools, name)?.clone();
    let (args, env) = call_parts(input);
    let invocation = plan_tool_call(&tool, &args, &env)?;
    Ok((tool, invocation))
}

/// What a foreground program did.
struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    elapsed: Duration,
    /// Why it was stopped, when it did not end by itself.
    stopped: Option<&'static str>,
}

/// Read a stream to its end on a thread, keeping at most [`MAX_STREAM_BYTES`].
fn drain(stream: Option<impl Read + Send + 'static>) -> JoinHandle<(Vec<u8>, usize)> {
    std::thread::spawn(move || {
        let (mut kept, mut dropped) = (Vec::new(), 0usize);
        let Some(mut stream) = stream else { return (kept, dropped) };
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            let room = MAX_STREAM_BYTES.saturating_sub(kept.len()).min(n);
            kept.extend_from_slice(&buf[..room]);
            dropped += n - room;
        }
        (kept, dropped)
    })
}

fn stream_text((bytes, dropped): (Vec<u8>, usize)) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if dropped > 0 {
        text.push_str(&format!("\n[… {dropped} more bytes were not kept …]"));
    }
    text
}

/// Run `inv` in `cwd` and wait for it, killing its tree at `limit` or when `cancelled` says so.
fn run_foreground(inv: &ToolInvocation, cwd: &Path, stdin: Option<String>, limit: Duration, cancelled: &dyn Fn() -> bool) -> Result<Finished, String> {
    let (program, args) = inv.argv.split_first().ok_or("nothing to run")?;
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd).envs(inv.env.iter().cloned());
    // Null unless there is something to send: a backend's own stdin is its protocol pipe.
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    own_group(&mut cmd);
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        cmd.no_window();
    }
    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|e| format!("could not start `{program}`: {e}"))?;
    let pid = child.id();
    arbor_process_ext::prelude::track_child_group(pid);
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // On its own thread: a program that writes before it reads would otherwise deadlock against us.
        std::thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
        });
    }
    let (out, err) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let mut stopped = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if started.elapsed() >= limit || cancelled() {
            stopped = Some(if cancelled() { "the run was cancelled" } else { "it reached its time limit" });
            kill_tree(&mut child);
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    arbor_process_ext::prelude::untrack_child_group(pid);
    let stdout = stream_text(out.join().unwrap_or_default());
    let stderr = stream_text(err.join().unwrap_or_default());
    Ok(Finished { code: status.and_then(|s| s.code()), stdout, stderr, elapsed: started.elapsed(), stopped })
}

/// The answer the model reads: how it ended, the command, then each stream that said something.
fn finished_text(name: &str, inv: &ToolInvocation, f: &Finished) -> String {
    let ending = match (f.stopped, f.code) {
        (Some(why), _) => format!("was stopped after {} because {why}", duration_text(f.elapsed.as_millis() as u64)),
        (None, Some(code)) => format!("exited with code {code} in {}", duration_text(f.elapsed.as_millis() as u64)),
        (None, None) => format!("ended by a signal after {}", duration_text(f.elapsed.as_millis() as u64)),
    };
    let mut out = format!("`{name}` {ending}.\n$ {}", display_argv(&inv.argv));
    let (o, e) = (f.stdout.trim_end(), f.stderr.trim_end());
    // The room goes to the streams that have something, three to one for stdout when both do.
    let (o_room, e_room) = match (o.is_empty(), e.is_empty()) {
        (false, false) => (TOOL_MAX_OUTPUT_CHARS * 3 / 4, TOOL_MAX_OUTPUT_CHARS / 4),
        _ => (TOOL_MAX_OUTPUT_CHARS, TOOL_MAX_OUTPUT_CHARS),
    };
    if !o.is_empty() {
        out.push_str(&format!("\n── stdout ──\n{}", clip_output(o, o_room)));
    }
    if !e.is_empty() {
        out.push_str(&format!("\n── stderr ──\n{}", clip_output(e, e_room)));
    }
    if o.is_empty() && e.is_empty() {
        out.push_str("\n(no output)");
    }
    out
}

impl ToolHost {
    pub(super) fn tool_cwd(&self, input: &Value) -> Result<PathBuf, String> {
        self.run_cwd(input)
    }

    /// The question a call raises, by the declaration (see the module's notes). `Err` is why it was refused.
    pub(super) fn external_gate(&self, input: &Value) -> Result<(), String> {
        let (tool, inv) = resolve(input)?;
        let key = format!("{}#tool:{}", conversation_key(&self.session.root, &self.session.conversation_id), tool.name);
        // A program (or a script handed to an interpreter) that lives in a project of the conversation is a file
        // Imber can edit: an approval is for its content too, or editing it and calling it again would run
        // anything under a yes given to something else.
        let stamp = self.content_stamp(&inv.argv);
        let hash = format!("{}{}", tool_fingerprint(&tool), stamp.as_deref().unwrap_or(""));
        let approved = approvals().is_recipe_approved(&key, &hash);
        let manual_write = tool.writes && !self.session.applies_edits();
        // Even a tool that never asks asks again when its file changed during the conversation.
        let seen_key = format!("{key}#seen");
        let changed = stamp.as_ref().is_some_and(|s| approvals().seen_hash(&seen_key).is_some_and(|seen| &seen != s));
        // Recorded only once the call may go: a "no" must leave the change to be asked about again.
        let seen = || {
            if let Some(s) = &stamp {
                approvals().remember_seen(&seen_key, s);
            }
        };
        if !manual_write && !changed && !tool_must_ask(&tool, approved) {
            seen();
            return Ok(());
        }
        let cwd = input.get("cwd").and_then(Value::as_str).unwrap_or(".");
        let env = inv.env.iter().filter(|(k, _)| !tool.env.contains_key(k)).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
        let mut detail = format!("$ {}\nin {cwd}", display_argv(&inv.argv));
        if !env.is_empty() {
            detail.push_str(&format!("\nwith {}", env.join(" ")));
        }
        if input.get("background").and_then(Value::as_bool).unwrap_or(false) {
            detail.push_str("\nIt keeps running in the background until it ends or is stopped.");
        }
        if changed {
            detail.push_str("\n\nIts file in the project has changed since it last ran in this conversation.");
        }
        let covers = tool.approval == ToolApproval::Conversation && !manual_write;
        if covers {
            let what = if stamp.is_some() { "the tool's settings or its file change" } else { "the tool's settings change" };
            detail.push_str(&format!("\n\nApproving covers every call of `{}` in this conversation, until {what}.", tool.name));
        }
        self.confirm(&format!("Run `{}`?", tool.name), &detail, tool.writes)?;
        seen();
        if covers {
            approvals().approve_recipes(&key, &hash);
        }
        Ok(())
    }

    /// A stamp of the content of the files `argv` names that live in a project of the conversation — the program,
    /// and a script handed to an interpreter — or `None` when it names none.
    pub(super) fn content_stamp(&self, argv: &[String]) -> Option<String> {
        let roots: Vec<PathBuf> = std::iter::once(self.session.root.clone()).chain(self.session.projects().into_iter().map(|p| p.root)).map(PathBuf::from).collect();
        let mut text = String::new();
        for word in argv {
            let path = Path::new(word);
            if path.is_absolute() && path.is_file() && roots.iter().any(|r| path.starts_with(r)) {
                let bytes = std::fs::read(path).ok()?;
                text.push_str(&format!("{word}\0{}\0", bennu_live::prelude::content_hash(&String::from_utf8_lossy(&bytes))));
            }
        }
        (!text.is_empty()).then(|| bennu_live::prelude::content_hash(&text))
    }

    /// The folder a call works in, for the access policy.
    pub(super) fn external_touched(&self, input: &Value) -> Vec<(PathBuf, bennu_assistant::prelude::Op)> {
        vec![(self.tool_cwd(input).unwrap_or_else(|_| self.project().to_path_buf()), bennu_assistant::prelude::Op::List)]
    }

    pub(super) fn run_external(&self, input: &Value) -> Result<String, String> {
        let (tool, inv) = resolve(input)?;
        let limit = Duration::from_secs(u64::from(tool.timeout_s));
        self.run_program(&tool.name, inv, input, limit)
    }

    /// Run `inv` for a call: in the background as a managed process when the call says so, otherwise to its end
    /// (or `limit`) with its output. Shared by `run_tool` and `run_script`.
    pub(super) fn run_program(&self, name: &str, inv: ToolInvocation, input: &Value, limit: Duration) -> Result<String, String> {
        let cwd = self.tool_cwd(input)?;
        if input.get("background").and_then(Value::as_bool).unwrap_or(false) {
            let name = input.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(name).to_string();
            let ready = match input.get("ready") {
                Some(v) if v.is_object() => readiness_from(v)?,
                _ => Readiness::default(),
            };
            let command = display_argv(&inv.argv);
            let spec = StartSpec { name, command, argv: inv.argv, cwd, env: inv.env, ports: Vec::new(), ready, group: None, lifetime: None, owner: None };
            return self.start_owned(&self.live(), spec, false);
        }
        let stdin = input.get("stdin").and_then(Value::as_str).map(|s| s.chars().take(MAX_STDIN_CHARS).collect::<String>());
        let cancelled = || self.session.cancelled();
        let finished = run_foreground(&inv, &cwd, stdin, limit, &cancelled)?;
        Ok(finished_text(name, &inv, &finished))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_program_runs_without_a_shell_and_its_streams_come_back() {
        let inv = ToolInvocation { argv: vec!["sh".into(), "-c".into(), "printf 'a;b'; printf oops >&2; exit 4".into()], env: vec![] };
        let f = run_foreground(&inv, &std::env::temp_dir(), None, Duration::from_secs(10), &|| false).unwrap();
        assert_eq!((f.code, f.stdout.as_str(), f.stderr.as_str(), f.stopped), (Some(4), "a;b", "oops", None));
        let text = finished_text("t", &inv, &f);
        assert!(text.starts_with("`t` exited with code 4") && text.contains("── stdout ──\na;b") && text.contains("── stderr ──\noops"), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn a_program_past_its_limit_is_killed_and_says_so() {
        let inv = ToolInvocation { argv: vec!["sleep".into(), "30".into()], env: vec![] };
        let f = run_foreground(&inv, &std::env::temp_dir(), None, Duration::from_millis(300), &|| false).unwrap();
        assert!(f.elapsed < Duration::from_secs(5));
        assert_eq!(f.stopped, Some("it reached its time limit"));
    }

    #[cfg(unix)]
    #[test]
    fn stdin_reaches_the_program() {
        let inv = ToolInvocation { argv: vec!["cat".into()], env: vec![] };
        let f = run_foreground(&inv, &std::env::temp_dir(), Some("hello".into()), Duration::from_secs(10), &|| false).unwrap();
        assert_eq!(f.stdout, "hello");
    }
}
