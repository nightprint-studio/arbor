//! agent — `.merula` checks and renders for an AI agent (Imber), reached through the shell's relay.
//!
//! The editor's own paths do not fit an agent. `merula_eval` stages the arrangement into the live session and
//! publishes its diagnostics to the editor, and `merula_render` reports only the first error and returns a job id
//! for the *Downloads & Exports* overlay. An agent has no overlay to watch: it needs **every** finding, with its
//! line, and it needs to know when the file is written. So:
//!
//! - [`merula_agent_check`] evaluates a file from disk and returns all of its diagnostics (language errors,
//!   instruments the registry does not know, speech controls on a non-speech source) with line and column. It
//!   stages nothing and publishes nothing: the editor's markers belong to what the user is editing.
//! - [`merula_agent_render_start`] checks the file the same way and, when it has no error, starts the bounce on its
//!   own thread and returns at once. The relay that carries this call runs on the shell's reader thread for the
//!   calling backend, so a call that waited for a render would freeze that backend whole for its length.
//! - [`merula_agent_render_status`] is how the caller waits: a quick read of where the render is, which also
//!   forgets a finished render once it has been read.
//! - [`merula_agent_render_cancel`] stops one, for a run the user stopped.
//!
//! The render is a tracked job like the editor's, so it shows in the overlay with its Stop button as well.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use merula::prelude::{render_offline_with_registry, RenderOutcome, RenderProgress};
use serde::{Deserialize, Serialize};

use merula_core::config as config_cmds;
use merula_core::events::Diagnostic;
use merula_core::prelude::MerulaState;

use crate::eval::validate;
use crate::jobs::{category, JobHandle};

// ── Arguments and results ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CheckArgs {
    /// The `.merula` file, absolute.
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct RenderArgs {
    /// The `.merula` file, absolute.
    pub path: String,
    /// Where to write the audio, absolute. Default: next to the source, same stem, the format's extension.
    #[serde(default)]
    pub out: Option<String>,
    /// `"ogg"` (the default) or `"wav"`.
    #[serde(default)]
    pub format: Option<String>,
    /// Cycles to render. Default: the file's `meta { cycles = "N" }`, else its arrangement's length, else 1.
    #[serde(default)]
    pub cycles: Option<u32>,
    #[serde(default)]
    pub start_cycle: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub normalize_lufs: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct RenderIdArgs {
    pub id: String,
}

/// One finding, placed in the file.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// `"error"` or `"warning"`.
    pub severity: String,
    pub message: String,
    /// 1-based, when the finding has a place.
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// The source line it points at, trimmed.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub path: String,
    /// No finding of severity `error`: the file evaluates and renders.
    pub ok: bool,
    pub findings: Vec<Finding>,
    /// The project folder imports resolve from (the nearest one with a `merula.toml`, else the file's folder).
    pub project: String,
    /// How many tracks the file produces (0 when it does not evaluate).
    pub tracks: usize,
    /// The render length the file implies, and the rule that gave it: `"meta"`, `"arrangement"` or `"default"`.
    pub cycles: u32,
    pub cycles_from: String,
    /// Tempo, cycles per second.
    pub cps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderStarted {
    /// `None` when the check found an error: nothing was started.
    pub id: Option<String>,
    pub out: String,
    pub format: String,
    pub cycles: u32,
    pub cycles_from: String,
    pub check: CheckReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderStatus {
    /// `"running"`, `"done"`, `"failed"`, `"cancelled"` or `"unknown"` (no render by that id).
    pub state: String,
    pub percent: i32,
    pub out: String,
    pub error: Option<String>,
    /// File size once written.
    pub bytes: Option<u64>,
    /// Length of the music rendered (the tail after it is not counted).
    pub seconds: Option<f64>,
    pub elapsed_ms: u64,
}

// ── Check ─────────────────────────────────────────────────────────────────────

/// Evaluate a `.merula` file and return every finding, placed (see the module doc).
#[arbor_rpc::handler]
fn merula_agent_check(ctx: &MerulaState, args: CheckArgs) -> Result<CheckReport, String> {
    Ok(check(ctx, Path::new(&args.path))?.0)
}

/// The check, and the evaluated arrangement when there is one to render.
fn check(ctx: &MerulaState, path: &Path) -> Result<(CheckReport, Option<merula::prelude::EvalOutput>), String> {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("merula")) {
        return Err(format!("{} is not a .merula file", path.display()));
    }
    let source = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let project = project_of(path);
    let cfg = config_cmds::load();
    let declared = merula::prelude::parse(&source).ok().and_then(|p| crate::export_all::meta_cycles(&p));

    let mut report = CheckReport {
        path: path.display().to_string(),
        ok: false,
        findings: Vec::new(),
        project: project.display().to_string(),
        tracks: 0,
        cycles: declared.filter(|n| *n > 0).unwrap_or(1),
        cycles_from: if declared.is_some_and(|n| n > 0) { "meta" } else { "default" }.into(),
        cps: None,
    };
    match crate::eval::evaluate_source(ctx.event_sink(), &source, project, cfg.eval_config()) {
        Ok(output) => {
            let known = validate::known_instruments(&cfg);
            let mut diags = validate::validate_instruments(&output.tracks, &known);
            diags.extend(validate::lint_speech_knobs(&source));
            report.findings = diags.iter().map(|d| place(&source, d)).collect();
            report.ok = !report.findings.iter().any(|f| f.severity == "error");
            report.tracks = output.tracks.tracks.len();
            report.cps = Some(crate::render::render_cps(&output, &cfg));
            let period = crate::export_all::arrangement_cycles(&output.tracks);
            if report.cycles_from == "default" && period > 0 {
                report.cycles = period;
                report.cycles_from = "arrangement".into();
            }
            Ok((report, Some(output)))
        }
        Err(diags) => {
            report.findings = diags.errors.iter().map(|d| place(&source, d)).collect();
            Ok((report, None))
        }
    }
}

/// The folder a file's imports and libraries resolve from: the nearest ancestor with a `merula.toml`, else the
/// file's own folder — the same root the editor uses when the file is opened in its project.
fn project_of(path: &Path) -> PathBuf {
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    folder.ancestors().find(|d| d.join("merula.toml").is_file()).map(Path::to_path_buf).unwrap_or(folder)
}

/// A diagnostic with its byte offset turned into a line, a column and the line's text.
fn place(source: &str, d: &Diagnostic) -> Finding {
    let at = d.start.map(|s| (s as usize).min(source.len()));
    let (line, column, text) = match at {
        Some(offset) => {
            let offset = (0..=offset).rev().find(|i| source.is_char_boundary(*i)).unwrap_or(0);
            let before = &source[..offset];
            let line = before.matches('\n').count() as u32 + 1;
            let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let column = source[line_start..offset].chars().count() as u32 + 1;
            let text = source[line_start..].lines().next().unwrap_or("").trim().to_string();
            (Some(line), Some(column), Some(text))
        }
        None => (None, None, None),
    };
    Finding { severity: d.severity.to_string(), message: d.message.clone(), line, column, text }
}

// ── Render ────────────────────────────────────────────────────────────────────

/// A render an agent started, as its thread updates it.
struct Running {
    state: String,
    percent: i32,
    out: String,
    error: Option<String>,
    bytes: Option<u64>,
    seconds: Option<f64>,
    started: Instant,
    cancel: Arc<AtomicBool>,
}

fn renders() -> &'static Mutex<HashMap<String, Running>> {
    static RENDERS: OnceLock<Mutex<HashMap<String, Running>>> = OnceLock::new();
    RENDERS.get_or_init(Default::default)
}

fn update(id: &str, change: impl FnOnce(&mut Running)) {
    if let Some(r) = renders().lock().unwrap_or_else(|p| p.into_inner()).get_mut(id) {
        change(r);
    }
}

/// Check a `.merula` file and, when it has no error, start rendering it (see the module doc).
#[arbor_rpc::handler]
fn merula_agent_render_start(ctx: &MerulaState, args: RenderArgs) -> Result<RenderStarted, String> {
    let path = PathBuf::from(&args.path);
    let (report, output) = check(ctx, &path)?;
    let ogg = args.format.as_deref().is_none_or(|f| f.eq_ignore_ascii_case("ogg"));
    let format = if ogg { "ogg" } else { "wav" };
    let out = args.out.map(PathBuf::from).unwrap_or_else(|| path.with_extension(format));
    let (cycles, cycles_from) = match args.cycles.filter(|n| *n > 0) {
        Some(n) => (n, "argument".to_string()),
        None => (report.cycles, report.cycles_from.clone()),
    };
    let mut started = RenderStarted { id: None, out: out.display().to_string(), format: format.into(), cycles, cycles_from, check: report };
    let Some(output) = output.filter(|_| started.check.ok) else {
        return Ok(started);
    };

    let cfg = config_cmds::load();
    let cps = crate::render::render_cps(&output, &cfg);
    let opts = crate::render::RenderOpts {
        cycles,
        start_cycle: args.start_cycle,
        bit_depth: None,
        tail_max_secs: None,
        sample_rate: args.sample_rate,
        format: Some(format.into()),
        normalize_lufs: args.normalize_lufs,
    };
    let render_cfg = crate::render::resolve_config(cfg.render.render_config(), &opts);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }

    let host = ctx.host_caller().ok_or_else(|| "merula_agent_render_start: no reverse channel".to_string())?;
    let name = format!("Render {}", out.display());
    let command = format!("render {cycles} cycles @ {cps} cps (for Imber)");
    let job = JobHandle::register(host, ctx.event_sink(), &name, &command, category::RENDERS)?;
    let id = job.id.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    renders().lock().unwrap_or_else(|p| p.into_inner()).insert(
        id.clone(),
        Running { state: "running".into(), percent: 0, out: started.out.clone(), error: None, bytes: None, seconds: None, started: Instant::now(), cancel: Arc::clone(&cancel) },
    );

    let tracks = output.tracks;
    let start_cycle = args.start_cycle.unwrap_or(0);
    let thread_id = id.clone();
    let spawn = std::thread::Builder::new().name(format!("merula-agent-render-{id}")).spawn(move || {
        let id = thread_id;
        // Decoding the voices is the slow part of a first render: on this thread, never the dispatcher's.
        let registry = crate::render::render_registry(&cfg, &tracks);
        let progress_job = job.clone_handle();
        let progress_id = id.clone();
        let mut last = -1;
        let on_progress = move |p: RenderProgress| {
            let pct = (p.fraction() * 100.0).round() as i32;
            if pct != last {
                last = pct;
                progress_job.emit_progress(pct);
                update(&progress_id, |r| r.percent = pct);
            }
        };
        let cancel_job = job.clone_handle();
        let should_cancel = move || cancel.load(Ordering::Relaxed) || cancel_job.is_cancelled();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_offline_with_registry(&tracks, cps, start_cycle, cycles, &render_cfg, &out, registry, on_progress, should_cancel)
        }));
        match outcome {
            Ok(Ok(RenderOutcome::Completed)) => {
                job.finish_ok();
                let bytes = std::fs::metadata(&out).ok().map(|m| m.len());
                update(&id, |r| {
                    r.state = "done".into();
                    r.percent = 100;
                    r.bytes = bytes;
                    r.seconds = (cps > 0.0).then(|| f64::from(cycles) / cps);
                });
            }
            Ok(Ok(RenderOutcome::Cancelled)) => {
                let _ = std::fs::remove_file(&out);
                job.finish_cancelled();
                update(&id, |r| r.state = "cancelled".into());
            }
            Ok(Err(e)) => fail(&job, &id, e.to_string()),
            Err(_) => fail(&job, &id, "the render thread panicked (see the log)".into()),
        }
    });
    if let Err(e) = spawn {
        renders().lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
        return Err(format!("failed to start the render thread: {e}"));
    }
    started.id = Some(id);
    Ok(started)
}

fn fail(job: &JobHandle, id: &str, msg: String) {
    job.append(&msg);
    job.finish_failed(msg.clone());
    update(id, |r| {
        r.state = "failed".into();
        r.error = Some(msg);
    });
}

/// Where a render is. A finished one is forgotten once read.
#[arbor_rpc::handler]
fn merula_agent_render_status(_ctx: &MerulaState, args: RenderIdArgs) -> Result<RenderStatus, String> {
    let mut all = renders().lock().unwrap_or_else(|p| p.into_inner());
    let Some(r) = all.get(&args.id) else {
        return Ok(RenderStatus { state: "unknown".into(), percent: 0, out: String::new(), error: None, bytes: None, seconds: None, elapsed_ms: 0 });
    };
    let status = RenderStatus {
        state: r.state.clone(),
        percent: r.percent,
        out: r.out.clone(),
        error: r.error.clone(),
        bytes: r.bytes,
        seconds: r.seconds,
        elapsed_ms: r.started.elapsed().as_millis() as u64,
    };
    if status.state != "running" {
        all.remove(&args.id);
    }
    Ok(status)
}

/// Ask a render to stop; it stops at its next block and removes the partial file.
#[arbor_rpc::handler]
fn merula_agent_render_cancel(_ctx: &MerulaState, args: RenderIdArgs) -> Result<(), String> {
    if let Some(r) = renders().lock().unwrap_or_else(|p| p.into_inner()).get(&args.id) {
        r.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(start: Option<u32>) -> Diagnostic {
        Diagnostic { message: "boom".into(), severity: "error", start, end: None }
    }

    #[test]
    fn a_finding_is_placed_on_its_line_and_column() {
        let src = "tempo(1)\n  drums = sound(\"bd\")\n";
        let f = place(src, &diag(Some(src.find("sound").unwrap() as u32)));
        assert_eq!((f.line, f.column), (Some(2), Some(11)));
        assert_eq!(f.text.as_deref(), Some("drums = sound(\"bd\")"));
    }

    #[test]
    fn a_finding_without_a_place_or_past_the_end_still_reads() {
        assert_eq!(place("x", &diag(None)).line, None);
        let f = place("a\nb", &diag(Some(99)));
        assert_eq!((f.line, f.column), (Some(2), Some(2)));
    }

    #[test]
    fn the_project_is_the_nearest_folder_with_a_manifest() {
        let tmp = std::env::temp_dir().join(format!("merula-agent-{}", std::process::id()));
        let nested = tmp.join("sfx/ui");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(tmp.join("merula.toml"), "").unwrap();
        assert_eq!(project_of(&nested.join("click.merula")), tmp);
        std::fs::remove_file(tmp.join("merula.toml")).unwrap();
        assert_eq!(project_of(&nested.join("click.merula")), nested);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
