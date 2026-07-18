//! `export_all` — bounce every `.merula` in a project to audio in one job.
//!
//! The Files panel can render one file at a time; a project like a game's audio
//! folder wants all of them, to the same format, in one go. Two handlers:
//!
//! - [`merula_export_plan`] lists the project's `.merula` files with the render
//!   length each one declares, so the modal can show a checklist without the user
//!   having to remember how long every asset is.
//! - [`merula_export_all`] renders the chosen subset into one output folder as a
//!   single tracked job.
//!
//! **Where the cycle count comes from.** A `Pattern` has no intrinsic length, so
//! every render needs an explicit cycle count. Rather than ask for 46 numbers, each
//! file declares its own in its front-matter — `meta { cycles = "88" }` — which is
//! the only place that knows it. Resolution order:
//!   1. `meta.cycles`, if present and parseable;
//!   2. the arrangement period (a file built from `arrange(...)` knows its own loop
//!      length via `Track::period`);
//!   3. `1` — the right default for the one-shot SFX that make up most of a game's
//!      audio folder.
//! The plan reports which rule fired so the modal can show it, and the caller may
//! override any entry.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use merula::prelude::{
    render_offline_with_progress, ControlMap, Item, MetaValue, Program, RenderOutcome,
    RenderProgress, Tracks,
};
use merula_core::config as config_cmds;
use merula_core::prelude::MerulaState;

use crate::jobs::{category, JobHandle};

/// One `.merula` in the export plan.
#[derive(Debug, Clone, Serialize)]
pub struct ExportPlanEntry {
    /// Absolute path to the source file.
    pub path: String,
    /// Project-relative path, for display.
    pub rel: String,
    /// The output stem (file name without `.merula`).
    pub stem: String,
    /// `meta.title`, when the file declares one.
    pub title: Option<String>,
    /// Cycles to render, resolved as documented above.
    pub cycles: u32,
    /// Which rule produced `cycles`: `"meta"` | `"arrangement"` | `"default"`.
    pub cycles_from: String,
    /// A parse/eval error, when the file can't be rendered as it stands. The modal
    /// shows these and excludes them by default; one broken file must not block the
    /// other forty-five.
    pub error: Option<String>,
}

/// One file the caller actually wants rendered.
#[derive(Debug, Clone, Deserialize)]
pub struct ExportEntry {
    pub path: String,
    pub stem: String,
    pub cycles: u32,
}

/// Format + quality for the whole batch. Deliberately ONE format for every file —
/// a game's audio folder wants a uniform set, and per-file formats are the kind of
/// choice that only produces mistakes.
#[derive(Debug, Clone, Deserialize)]
pub struct ExportAllOpts {
    /// `"wav"` | `"ogg"`.
    pub format: String,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<String>,
    pub tail_max_secs: Option<f32>,
    #[serde(default)]
    pub normalize_lufs: Option<f32>,
}

/// Read `meta { cycles = "N" }` from a parsed program, if it declares one.
fn meta_cycles(program: &Program) -> Option<u32> {
    program.items.iter().find_map(|item| {
        let Item::Meta(block) = item else { return None };
        block.fields.iter().find_map(|f| {
            if f.key.name != "cycles" {
                return None;
            }
            match &f.value {
                MetaValue::Str(s) => s.trim().parse::<u32>().ok(),
                _ => None,
            }
        })
    })
}

/// The longest arrangement period across a file's tracks (`0` when no track is an
/// `arrange(...)`), which is that file's natural loop length.
fn arrangement_cycles(tracks: &Tracks<ControlMap>) -> u32 {
    tracks.tracks.iter().map(|t| t.period).max().unwrap_or(0)
}

/// List the project's `.merula` files with a resolved render length for each.
#[arbor_rpc::handler]
fn merula_export_plan(ctx: &MerulaState, dir: String) -> Result<Vec<ExportPlanEntry>, String> {
    let cfg = config_cmds::load();
    let root = PathBuf::from(&dir);
    let mut files = Vec::new();
    collect_merula(&root, &mut files);
    files.sort();

    let mut out = Vec::with_capacity(files.len());
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "out".to_string());

        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                out.push(plan_error(&path, &rel, &stem, format!("read: {e}")));
                continue;
            }
        };

        // Parse first: `meta.cycles` is available even if evaluation later fails.
        let (declared, title) = match merula::prelude::parse(&source) {
            Ok(p) => (meta_cycles(&p), meta_title(&p)),
            Err(e) => {
                out.push(plan_error(&path, &rel, &stem, format!("parse: {e}")));
                continue;
            }
        };

        // Evaluate to surface errors now (better in a checklist than mid-batch) and
        // to learn the arrangement period when the file declares no `cycles`.
        let evaluated = crate::eval::evaluate_source(
            ctx.event_sink(),
            &source,
            root.clone(),
            cfg.eval_config(),
        );
        let (cycles, cycles_from, error) = match evaluated {
            Ok(output) => {
                let period = arrangement_cycles(&output.tracks);
                match (declared, period) {
                    (Some(n), _) if n > 0 => (n, "meta", None),
                    (_, p) if p > 0 => (p, "arrangement", None),
                    _ => (1, "default", None),
                }
            }
            Err(diags) => {
                let msg = diags
                    .errors
                    .first()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "evaluation failed".to_string());
                (declared.unwrap_or(1), "meta", Some(msg))
            }
        };

        out.push(ExportPlanEntry {
            path: path.to_string_lossy().to_string(),
            rel,
            stem,
            title,
            cycles,
            cycles_from: cycles_from.to_string(),
            error,
        });
    }
    Ok(out)
}

/// `meta.title`, when present.
fn meta_title(program: &Program) -> Option<String> {
    program.items.iter().find_map(|item| {
        let Item::Meta(block) = item else { return None };
        block.fields.iter().find_map(|f| match (&f.key.name[..], &f.value) {
            ("title", MetaValue::Str(s)) => Some(s.clone()),
            _ => None,
        })
    })
}

/// A plan row for a file that can't be read or parsed at all.
fn plan_error(path: &Path, rel: &str, stem: &str, msg: String) -> ExportPlanEntry {
    ExportPlanEntry {
        path: path.to_string_lossy().to_string(),
        rel: rel.to_string(),
        stem: stem.to_string(),
        title: None,
        cycles: 1,
        cycles_from: "default".to_string(),
        error: Some(msg),
    }
}

/// Render every chosen file into `out_dir`, all to the same format, as ONE job.
/// Returns the job id; progress spans the whole batch and cancellation stops between
/// files (those already written are complete and are left in place).
#[arbor_rpc::handler]
fn merula_export_all(
    ctx: &MerulaState,
    dir: String,
    entries: Vec<ExportEntry>,
    out_dir: String,
    opts: ExportAllOpts,
) -> Result<String, String> {
    if entries.is_empty() {
        return Err("nothing selected to export".to_string());
    }
    let cfg = config_cmds::load();
    let root = PathBuf::from(&dir);
    let out_root = PathBuf::from(&out_dir);
    std::fs::create_dir_all(&out_root).map_err(|e| format!("create {out_dir}: {e}"))?;

    // Evaluate everything up front, on this worker: evaluation needs the event sink
    // and is fast, while the renders are the slow part and belong on their own
    // thread. A file that fails here is reported and skipped, not fatal — one bad
    // asset must not cost the batch.
    let mut jobs: Vec<(String, Tracks<ControlMap>, f64, u32)> =
        Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for e in &entries {
        let source = match std::fs::read_to_string(&e.path) {
            Ok(s) => s,
            Err(err) => {
                failed.push(format!("{}: read: {err}", e.stem));
                continue;
            }
        };
        match crate::eval::evaluate_source(ctx.event_sink(), &source, root.clone(), cfg.eval_config())
        {
            Ok(output) => {
                let cps = crate::render::render_cps(&output, &cfg);
                jobs.push((e.stem.clone(), output.tracks, cps, e.cycles.max(1)));
            }
            Err(diags) => {
                let msg = diags
                    .errors
                    .first()
                    .map(|d| d.message.clone())
                    .unwrap_or_else(|| "evaluation failed".to_string());
                failed.push(format!("{}: {msg}", e.stem));
            }
        }
    }
    if jobs.is_empty() {
        return Err(format!("every selected file failed to evaluate: {}", failed.join("; ")));
    }

    let render_cfg = crate::render::resolve_export_config(cfg.render.render_config(), &opts);
    let ext = if opts.format.eq_ignore_ascii_case("ogg") { "ogg" } else { "wav" };

    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_export_all: no reverse channel".to_string())?;
    let total = jobs.len();
    let job = JobHandle::register(
        host,
        ctx.event_sink(),
        &format!("Export {total} files"),
        &format!("export {total} → {} ({ext})", out_root.display()),
        category::RENDERS,
    )?;
    let job_id = job.id.clone();

    // Plain OS thread: rendering is blocking CPU/IO work, never the dispatcher worker.
    let spawn = std::thread::Builder::new()
        .name(format!("merula-export-all-{job_id}"))
        .spawn(move || {
            for msg in &failed {
                job.append(&format!("skipped {msg}"));
            }
            let mut done = 0usize;
            let mut errors = failed.len();
            for (i, (stem, tracks, cps, cycles)) in jobs.into_iter().enumerate() {
                if job.is_cancelled() {
                    job.append(&format!("cancelled after {done}/{total}"));
                    job.finish_cancelled();
                    return;
                }
                let out_path = out_root.join(format!("{stem}.{ext}"));
                job.append(&format!("[{}/{total}] {stem} — {cycles} cycles", i + 1));

                // Progress spans the batch: each file contributes its slice, so the
                // bar reflects the whole export rather than restarting per file.
                let base = (i as f32 / total as f32 * 100.0) as i32;
                let span = 100.0 / total as f32;
                let progress_job = job.clone_handle();
                let mut last = -1i32;
                let on_progress = move |p: RenderProgress| {
                    let pct = base + (p.fraction() * span) as i32;
                    if pct != last {
                        last = pct;
                        progress_job.emit_progress(pct);
                    }
                };
                let cancel_job = job.clone_handle();
                let should_cancel = move || cancel_job.is_cancelled();

                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    render_offline_with_progress(
                        &tracks, cps, 0, cycles, &render_cfg, &out_path, on_progress,
                        should_cancel,
                    )
                }));
                match outcome {
                    Ok(Ok(RenderOutcome::Completed)) => done += 1,
                    Ok(Ok(RenderOutcome::Cancelled)) => {
                        let _ = std::fs::remove_file(&out_path);
                        job.append(&format!("cancelled after {done}/{total}"));
                        job.finish_cancelled();
                        return;
                    }
                    Ok(Err(e)) => {
                        errors += 1;
                        job.append(&format!("  failed: {e}"));
                    }
                    Err(_) => {
                        errors += 1;
                        job.append("  failed: render thread panicked");
                    }
                }
            }
            job.emit_progress(100);
            if errors > 0 {
                // Partial success is still success: the files that rendered are on
                // disk and usable. The count is what the user needs to see.
                job.append(&format!("exported {done}/{total}, {errors} failed"));
            } else {
                job.append(&format!("exported {done}/{total}"));
            }
            job.finish_ok();
        });
    if let Err(e) = spawn {
        return Err(format!("failed to spawn export thread: {e}"));
    }
    Ok(job_id)
}

/// Recursively collect `*.merula` under `dir`.
fn collect_merula(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_merula(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("merula")) {
            out.push(path);
        }
    }
}
