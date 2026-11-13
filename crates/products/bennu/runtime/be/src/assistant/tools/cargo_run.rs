//! `cargo_run`: run a binary or an example of the project with features, arguments and environment, as a managed
//! process of the Live family — so `run_logs`, `wait_ready`, `http_request` and `run_stop` work on it like on
//! anything `run_start` started.
//!
//! The model never writes the command line: the call's fields become an [`Invocation`] and the argument vector is
//! the one [`argv`] builds for the Rust tool window's run configurations, so what runs here and what a
//! configuration with the same fields runs cannot drift. It goes straight to cargo, with no shell.
//!
//! ## What asks the user
//!
//! The first run of a target (package + bin or example) in a conversation. The yes covers that target again with
//! any features, arguments or environment: they change how the program runs, not which program runs. Another
//! target asks again. A scheduled run never starts one.

use bennu_assistant::prelude::ToolSpec;
use bennu_cargo::prelude::{argv, display_command, Invocation, TargetSelector};
use std::time::{Duration, Instant};

use bennu_live::prelude::{LogLine, LogQuery, Manager, Readiness, StartSpec, Stream};
use serde_json::{json, Value};

use super::live::readiness_from;
use super::{spec, ToolHost};
use crate::cargo_cmd::cargo_launcher;
use crate::live::{approvals, conversation_key};

pub(super) const NAME: &str = "cargo_run";

/// How long `until_exit` waits by default, and at most.
const DEFAULT_UNTIL_EXIT_S: u64 = 300;
const MAX_UNTIL_EXIT_S: u64 = 1800;
/// How often it looks whether the program has ended.
const POLL: Duration = Duration::from_millis(200);
/// The last lines of output an ended program's answer carries, and the most characters of them.
const TAIL_LINES: usize = 200;
const TAIL_CHARS: usize = 12_000;

/// Approvals here are per target, not per content: there is no file whose change should ask again.
const APPROVAL_HASH: &str = "target";

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        NAME,
        "Build and run a binary or an example of this Rust project (`cargo run`) as a managed process, and return \
         at once: read its output with run_logs (or wait_ready / `wait`), stop it with run_stop. Pick features, \
         profile, the program's own arguments and environment variables with the fields below — never a command \
         line. The user approves the first run of each target in a conversation; later runs of it, with any \
         features or arguments, do not ask. A build error shows in run_logs and the process ends. For a program \
         that ends by itself, `until_exit` waits for it and returns its exit code and output.",
        json!({"type":"object","properties":{
            "package":{"type":"string","description":"`-p`: the crate. Default: the one the root manifest names."},
            "bin":{"type":"string","description":"`--bin`: which binary, when the crate has several."},
            "example":{"type":"string","description":"`--example`: run an example instead of a binary."},
            "features":{"type":"array","items":{"type":"string"},"description":"`--features`."},
            "no_default_features":{"type":"boolean"},
            "all_features":{"type":"boolean"},
            "release":{"type":"boolean"},
            "profile":{"type":"string","description":"`--profile <name>`; wins over `release`."},
            "args":{"type":"array","items":{"type":"string"},"description":"The program's own arguments (after `--`), one per item."},
            "env":{"type":"object","additionalProperties":{"type":"string"},"description":"Environment variables for the build and the program (RUST_LOG, …)."},
            "cwd":{"type":"string","description":"Where cargo runs, which decides the workspace: a folder of the project, or another project of the conversation by name (`voriz:` or `voriz:crates/app`). Default: the project root."},
            "name":{"type":"string","description":"The process's name for run_logs/run_stop. Default: the target's name."},
            "ready":{"type":"object","description":"When it is ready, as in run_start (port, log, http, status, delay_ms, timeout_s)."},
            "wait":{"type":"boolean","description":"Wait until ready before returning (needs `ready`). Default false."},
            "until_exit":{"type":"boolean","description":"For a program that does its work and ends (a CLI, a tool, a one-shot): wait until it exits and return its exit code and the last lines of its output, build errors included. Default false: return at once, as for a server."},
            "timeout_s":{"type":"integer","description":"With until_exit: the longest to wait, in seconds (default 300, at most 1800). Past it the program keeps running and run_logs reads it."}
        }}),
    )]
}

fn text(input: &Value, key: &str) -> String {
    input.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("").to_string()
}

fn words(input: &Value, key: &str) -> Vec<String> {
    input.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn flag(input: &Value, key: &str) -> bool {
    input.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// The `cargo run` a call describes.
fn invocation(input: &Value) -> Result<Invocation, String> {
    let (bin, example) = (text(input, "bin"), text(input, "example"));
    if !bin.is_empty() && !example.is_empty() {
        return Err("give `bin` or `example`, not both".into());
    }
    let target = match (bin.is_empty(), example.is_empty()) {
        (false, _) => TargetSelector { kind: "bin".into(), name: bin },
        (_, false) => TargetSelector { kind: "example".into(), name: example },
        _ => TargetSelector::default(),
    };
    let args = words(input, "args");
    if args.iter().any(|a| a.contains('\0')) {
        return Err("an argument contains a NUL character".into());
    }
    Ok(Invocation {
        command: "run".into(),
        package: text(input, "package"),
        target,
        release: flag(input, "release"),
        profile: text(input, "profile"),
        features: words(input, "features").into_iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect(),
        all_features: flag(input, "all_features"),
        no_default_features: flag(input, "no_default_features"),
        args,
        ..Default::default()
    })
}

/// What the approval is for: the package and the target.
fn target_label(inv: &Invocation) -> String {
    let package = if inv.package.is_empty() { "the default package" } else { inv.package.as_str() };
    match inv.target.kind.as_str() {
        "" => package.to_string(),
        kind => format!("{package}, {kind} `{}`", inv.target.name),
    }
}

impl ToolHost {
    /// The first run of a target in this conversation asks; the yes covers it with any features or arguments.
    pub(super) fn cargo_run_gate(&self, input: &Value) -> Result<(), String> {
        let inv = invocation(input)?;
        let key = format!(
            "{}#cargo-run:{}:{}:{}",
            conversation_key(&self.session.root, &self.session.conversation_id),
            inv.package,
            inv.target.kind,
            inv.target.name
        );
        if approvals().is_recipe_approved(&key, APPROVAL_HASH) {
            return Ok(());
        }
        let cwd = input.get("cwd").and_then(Value::as_str).unwrap_or(".");
        let detail = format!(
            "$ {}\nin {cwd}\n\nApproving covers running {} again in this conversation, with any features, arguments or environment.",
            display_command(&inv),
            target_label(&inv)
        );
        self.confirm("Run the program?", &detail, true)?;
        approvals().approve_recipes(&key, APPROVAL_HASH);
        Ok(())
    }

    pub(super) fn run_cargo_run(&self, input: &Value) -> Result<String, String> {
        let inv = invocation(input)?;
        let cwd = self.run_cwd(input)?;
        let name = match text(input, "name") {
            n if !n.is_empty() => n,
            _ if !inv.target.name.is_empty() => inv.target.name.clone(),
            _ if !inv.package.is_empty() => inv.package.clone(),
            _ => "cargo-run".into(),
        };
        let ready = match input.get("ready") {
            Some(v) if v.is_object() => readiness_from(v)?,
            _ => Readiness::default(),
        };
        // A log has no cursor: no colours to strip, no progress bar redrawn hundreds of times.
        let mut env = vec![("CARGO_TERM_COLOR".to_string(), "never".to_string()), ("CARGO_TERM_PROGRESS_WHEN".to_string(), "never".to_string())];
        if let Some(o) = input.get("env").and_then(Value::as_object) {
            env.extend(o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))));
        }
        let argv: Vec<String> = std::iter::once(cargo_launcher().to_string_lossy().into_owned()).chain(argv(&inv)).collect();
        let spec = StartSpec { name, command: display_command(&inv), argv, cwd, env, ports: Vec::new(), ready, group: None, lifetime: None, owner: None };
        let live = self.live();
        let (info, out) = self.start_owned_info(&live, spec, flag(input, "wait"))?;
        if !flag(input, "until_exit") {
            return Ok(format!("{out}\nIt builds first: a compile error shows in run_logs and the process ends."));
        }
        let limit = Duration::from_secs(input.get("timeout_s").and_then(Value::as_u64).unwrap_or(DEFAULT_UNTIL_EXIT_S).clamp(1, MAX_UNTIL_EXIT_S));
        self.until_exit(&live.manager, &info.id, limit)
    }

    /// Wait for a program to end and say how: its exit code, and the last lines it printed. Past `limit` it is left
    /// running, and the answer says so.
    fn until_exit(&self, manager: &Manager, id: &str, limit: Duration) -> Result<String, String> {
        let started = Instant::now();
        let info = loop {
            let info = manager.get(id)?;
            if info.phase.is_finished() || self.session.cancelled() || started.elapsed() >= limit {
                break info;
            }
            std::thread::sleep(POLL);
        };
        let (_, lines) = manager.logs(id, &LogQuery { tail: Some(TAIL_LINES), ..Default::default() })?;
        let head = match (info.phase.is_finished(), info.exit_code) {
            (true, Some(0)) => format!("`{}` exited with code 0 after {}s.", info.name, started.elapsed().as_secs()),
            (true, Some(code)) => format!("`{}` exited with code {code} after {}s (a build error ends it before the program starts).", info.name, started.elapsed().as_secs()),
            (true, None) => format!("`{}` ended without an exit code (killed or stopped){}.", info.name, info.note.as_deref().map(|n| format!(": {n}")).unwrap_or_default()),
            (false, _) => format!("`{}` is still running after {}s: it keeps going — read it with run_logs, stop it with run_stop.", info.name, started.elapsed().as_secs()),
        };
        Ok(format!("{head}\n{}", output_tail(&lines)))
    }
}

/// The last lines of output, stderr marked, cut from the front to fit: the end is where a failure says why.
fn output_tail(lines: &[LogLine]) -> String {
    if lines.is_empty() {
        return "It printed nothing.".into();
    }
    let text = lines
        .iter()
        .map(|l| if matches!(l.stream, Stream::Stderr) { format!("! {}", l.text) } else { l.text.clone() })
        .collect::<Vec<_>>()
        .join("\n");
    let count = text.chars().count();
    let shown: String = if count > TAIL_CHARS { text.chars().skip(count - TAIL_CHARS).collect() } else { text };
    let cut = if count > TAIL_CHARS { " (the start is cut: run_logs has all of it)" } else { "" };
    format!("Last lines of its output{cut}, stderr marked `!`:\n{shown}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_becomes_the_cargo_run_a_configuration_would_be() {
        let inv = invocation(&json!({"package": "game", "bin": "client", "features": ["dev", " "], "release": true, "args": ["--level", "3"]})).unwrap();
        let line = argv(&inv);
        assert_eq!(line.first().map(String::as_str), Some("run"));
        for w in ["-p", "game", "--bin", "client", "--release", "--features", "dev", "--", "--level", "3"] {
            assert!(line.iter().any(|a| a == w), "{w} missing from {line:?}");
        }
        assert_eq!(target_label(&inv), "game, bin `client`");
    }

    #[test]
    fn an_ended_program_brings_its_last_lines_with_stderr_marked() {
        let line = |seq, stream, text: &str| LogLine { seq, ts_ms: 0, stream, text: text.into() };
        let out = output_tail(&[line(1, Stream::Stdout, "done: 3 files"), line(2, Stream::Stderr, "warning: slow disk")]);
        assert!(out.contains("done: 3 files\n! warning: slow disk"), "{out}");
        assert_eq!(output_tail(&[]), "It printed nothing.");
        let long: Vec<LogLine> = (0..2_000).map(|i| line(i, Stream::Stdout, "0123456789")).collect();
        let cut = output_tail(&long);
        assert!(cut.contains("the start is cut") && cut.chars().count() < TAIL_CHARS + 200);
    }

    #[test]
    fn bin_and_example_together_are_refused() {
        assert!(invocation(&json!({"bin": "a", "example": "b"})).unwrap_err().contains("not both"));
        assert_eq!(target_label(&invocation(&json!({})).unwrap()), "the default package");
    }
}
