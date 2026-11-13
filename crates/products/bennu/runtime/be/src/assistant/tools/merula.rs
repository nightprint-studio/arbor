//! `.merula` files: checking one and rendering it to audio, through Merula.
//!
//! Merula owns the language, the sound registry and the renderer, so the model reaches `merula-be` through the
//! shell (`__assistant_merula`, an allowlist of four calls) the way it reaches git. A check returns every finding
//! with its line; a render checks first, then starts the bounce on Merula's side and is waited for here, by
//! polling — the relay runs on this backend's reader thread in the shell, so no call through it may last.
//!
//! The model cannot listen to what it renders: the answer says what it can know instead (the length, the size,
//! the findings), and the user hears the file.

use std::time::{Duration, Instant};

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, ToolHost};

pub(super) const CHECK: &str = "merula_check";
pub(super) const RENDER: &str = "merula_render";

/// How often a render in progress is asked where it is.
const POLL: Duration = Duration::from_millis(250);
/// A render still going after this is stopped: a few minutes of audio renders in seconds, so this is a hang.
const MAX_RENDER: Duration = Duration::from_secs(15 * 60);

pub(super) fn is_merula_tool(name: &str) -> bool {
    name == CHECK || name == RENDER
}

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            CHECK,
            "Check a `.merula` file (Merula's live-coding music language) the way Merula's editor does, without playing it: every error and warning with its line and column — syntax, evaluation, instruments or samples the sound registry does not know (they would play on the fallback synth), speech controls on a non-speech source. Also says how many tracks it makes and the render length it implies. Use it after editing a .merula file, before rendering.",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"The .merula file, absolute or relative to the project (or `project:path`)."}
            },"required":["path"]}),
        ),
        spec(
            RENDER,
            "Render a `.merula` file to audio with Merula (OGG by default, or WAV), and wait for it. It is checked first: with an error nothing is rendered and you get the findings, placed, as merula_check gives them. Otherwise you get the written file, its length and size. The length defaults to the file's `meta { cycles = \"N\" }`, else its arrangement's length, else one cycle. You cannot hear the result: tell the user where it is.",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"The .merula file, absolute or relative to the project."},
                "out":{"type":"string","description":"Where to write the audio. Default: next to the source, same name, with the format's extension."},
                "format":{"type":"string","enum":["ogg","wav"],"description":"Default ogg."},
                "cycles":{"type":"integer","minimum":1,"description":"How many cycles to render, when the file's own length is not what you want."},
                "start_cycle":{"type":"integer","minimum":0,"description":"First cycle to render (a section). Default 0."},
                "sample_rate":{"type":"integer","description":"Default: Merula's render setting."},
                "normalize_lufs":{"type":"number","description":"Normalize to this integrated loudness (e.g. -16). Default: Merula's render setting."}
            },"required":["path"]}),
        ),
    ]
}

impl ToolHost {
    pub(super) fn run_merula(&self, name: &str, input: &Value) -> Result<String, String> {
        let raw = input.get("path").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or("`path` is required")?;
        let path = self.resolve(raw);
        if name == CHECK {
            let report = self.merula("merula_agent_check", json!({ "path": path.display().to_string() }))?;
            return Ok(check_text(&report));
        }
        let mut args = json!({ "path": path.display().to_string() });
        for key in ["format", "cycles", "start_cycle", "sample_rate", "normalize_lufs"] {
            if let Some(v) = input.get(key).filter(|v| !v.is_null()) {
                args[key] = v.clone();
            }
        }
        if let Some(out) = self.merula_out(input) {
            args["out"] = Value::String(out.display().to_string());
        }
        let started = self.merula("merula_agent_render_start", args)?;
        let check = started.get("check").cloned().unwrap_or(Value::Null);
        let Some(id) = started.get("id").and_then(Value::as_str) else {
            return Ok(format!("Not rendered: the file has errors.\n{}", check_text(&check)));
        };
        let status = self.wait_render(id)?;
        Ok(render_text(&started, &status, &check))
    }

    /// The output path the call names, resolved like the source; `None` for the default (next to the source).
    pub(super) fn merula_out(&self, input: &Value) -> Option<std::path::PathBuf> {
        input.get("out").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(|o| self.resolve(o))
    }

    /// Where a render writes, for the gate: the named file, or the source's sibling with the format's extension.
    pub(super) fn merula_target(&self, input: &Value) -> Option<std::path::PathBuf> {
        if let Some(out) = self.merula_out(input) {
            return Some(out);
        }
        let source = self.resolve(input.get("path").and_then(Value::as_str)?);
        let wav = input.get("format").and_then(Value::as_str).is_some_and(|f| f.eq_ignore_ascii_case("wav"));
        Some(source.with_extension(if wav { "wav" } else { "ogg" }))
    }

    /// A render asks as the project's own build does — every call, once per run, or never, by the mode — and an
    /// approval of one counts for the other: both turn the project's sources into files.
    pub(super) fn merula_render_gate(&self, input: &Value) -> Result<(), String> {
        if !bennu_assistant::prelude::must_ask_own_command(self.session.mode, self.session.own_commands_approved()) {
            return Ok(());
        }
        let source = input.get("path").and_then(Value::as_str).unwrap_or("?");
        let target = self.merula_target(input).map(|p| p.display().to_string()).unwrap_or_default();
        self.confirm("Render with Merula?", &format!("{source} → {target}"), false)?;
        self.session.approve_own_commands();
        Ok(())
    }

    fn merula(&self, method: &str, args: Value) -> Result<Value, String> {
        self.session.ctx.host_call("__assistant_merula", json!({ "method": method, "args": args })).map_err(|e| unreachable_merula(&e))
    }

    /// Poll a render until it ends. A stopped run stops it; one past [`MAX_RENDER`] is stopped as a hang.
    fn wait_render(&self, id: &str) -> Result<Value, String> {
        let began = Instant::now();
        let mut asked_to_stop = false;
        loop {
            let status = self.merula("merula_agent_render_status", json!({ "id": id }))?;
            if status.get("state").and_then(Value::as_str) != Some("running") {
                return Ok(status);
            }
            if !asked_to_stop && (self.session.cancelled() || began.elapsed() >= MAX_RENDER) {
                self.merula("merula_agent_render_cancel", json!({ "id": id }))?;
                asked_to_stop = true;
            }
            std::thread::sleep(POLL);
        }
    }
}

/// Merula not answering says why it might not, rather than the relay's own words.
fn unreachable_merula(e: &str) -> String {
    if e.contains("not running") || e.contains("UnknownMethod") || e.contains("unknown method") || e.contains("disconnected") {
        format!("Merula could not be reached ({e}). Its backend may be missing from this build of Arbor.")
    } else {
        e.to_string()
    }
}

/// A check report as the model reads it: the verdict, then each finding at its place.
fn check_text(report: &Value) -> String {
    let s = |k: &str| report.get(k).and_then(Value::as_str).unwrap_or("");
    let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let tracks = report.get("tracks").and_then(Value::as_u64).unwrap_or(0);
    let cycles = report.get("cycles").and_then(Value::as_u64).unwrap_or(1);
    let mut out = format!("{}: {}", s("path"), if ok { "no errors" } else { "has errors" });
    if ok {
        out.push_str(&format!(" — {tracks} track(s), renders {cycles} cycle(s) (from {})", cycles_rule(s("cycles_from"))));
        if let Some(cps) = report.get("cps").and_then(Value::as_f64).filter(|c| *c > 0.0) {
            out.push_str(&format!(" at {cps} cps, about {:.1}s", cycles as f64 / cps));
        }
    }
    out.push('.');
    let findings = report.get("findings").and_then(Value::as_array).cloned().unwrap_or_default();
    for f in &findings {
        let at = match (f.get("line").and_then(Value::as_u64), f.get("column").and_then(Value::as_u64)) {
            (Some(l), Some(c)) => format!(" {l}:{c}"),
            _ => String::new(),
        };
        let severity = f.get("severity").and_then(Value::as_str).unwrap_or("error");
        let message = f.get("message").and_then(Value::as_str).unwrap_or("");
        out.push_str(&format!("\n{severity}{at}: {message}"));
        if let Some(text) = f.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            out.push_str(&format!("\n    | {text}"));
        }
    }
    if !findings.is_empty() || !ok {
        out.push_str(&format!("\nImports resolve from {}.", s("project")));
    }
    out
}

fn cycles_rule(from: &str) -> &str {
    match from {
        "meta" => "the file's meta cycles",
        "arrangement" => "its arrangement's length",
        "argument" => "your `cycles`",
        _ => "the default, one cycle",
    }
}

/// How a render ended, with what the model can know about the file.
fn render_text(started: &Value, status: &Value, check: &Value) -> String {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let out = s(status, "out");
    let secs = status.get("elapsed_ms").and_then(Value::as_u64).unwrap_or(0) as f64 / 1000.0;
    let warnings = check.get("findings").and_then(Value::as_array).map_or(0, Vec::len);
    let after = if warnings > 0 { format!("\n{}", check_text(check)) } else { String::new() };
    match s(status, "state").as_str() {
        "done" => {
            let bytes = status.get("bytes").and_then(Value::as_u64).map(|b| format!(", {}", size(b))).unwrap_or_default();
            let length = status.get("seconds").and_then(Value::as_f64).map(|l| format!(", {l:.1}s of music plus its tail")).unwrap_or_default();
            let cycles = started.get("cycles").and_then(Value::as_u64).unwrap_or(1);
            let rule = cycles_rule(&s(started, "cycles_from")).to_string();
            format!(
                "Rendered {} ({}) — {cycles} cycle(s), from {rule}{length}{bytes}, in {secs:.1}s.{after}",
                out,
                s(started, "format").to_uppercase()
            )
        }
        "cancelled" => format!("The render of {out} was stopped after {secs:.1}s; the partial file was removed."),
        "failed" => format!("The render of {out} failed: {}", s(status, "error")),
        other => format!("The render of {out} ended in an unexpected state `{other}`."),
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1_000_000 => format!("{:.1} MB", b as f64 / 1_000_000.0),
        b if b >= 1_000 => format!("{} KB", b / 1_000),
        b => format!("{b} bytes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_with_errors_lists_each_at_its_place() {
        let report = json!({
            "path": "/p/a.merula", "ok": false, "project": "/p", "tracks": 0, "cycles": 1, "cycles_from": "default",
            "findings": [{"severity":"error","message":"unknown instrument `pianoo`","line":3,"column":9,"text":"lead = note(\"c\").s(\"pianoo\")"}]
        });
        let text = check_text(&report);
        assert!(text.starts_with("/p/a.merula: has errors."), "{text}");
        assert!(text.contains("error 3:9: unknown instrument `pianoo`\n    | lead"), "{text}");
        assert!(text.ends_with("Imports resolve from /p."), "{text}");
    }

    #[test]
    fn a_clean_check_says_its_length() {
        let report = json!({"path":"a.merula","ok":true,"project":"/p","tracks":2,"cycles":8,"cycles_from":"meta","cps":0.5,"findings":[]});
        assert_eq!(check_text(&report), "a.merula: no errors — 2 track(s), renders 8 cycle(s) (from the file's meta cycles) at 0.5 cps, about 16.0s.");
    }

    #[test]
    fn a_finished_render_names_the_file_and_its_size() {
        let started = json!({"format":"ogg","cycles":4,"cycles_from":"arrangement"});
        let status = json!({"state":"done","out":"/p/a.ogg","bytes":120_000,"seconds":8.0,"elapsed_ms":1500});
        let text = render_text(&started, &status, &json!({"findings": []}));
        assert_eq!(text, "Rendered /p/a.ogg (OGG) — 4 cycle(s), from its arrangement's length, 8.0s of music plus its tail, 120 KB, in 1.5s.");
    }

    #[test]
    fn both_tools_are_merula_tools() {
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert!(names.iter().all(|n| is_merula_tool(n)) && names.len() == 2);
    }
}
