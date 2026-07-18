//! render — offline render to WAV / stems / MIDI + clip-level analysis, on a
//! background job (or the dispatcher worker for the quick, file-IO-only walks).
//!
//! Ported from `src-tauri/src/merula/render.rs` + the four render/export commands
//! in `src-tauri/src/merula/mod.rs`. The heavy bounces (`render_offline*`) are
//! synchronous CPU/IO work, so they run on a plain detached `std::thread` and drive
//! the shell's `JobRegistry` over the reverse channel via [`JobHandle`] — never the
//! audio RT thread, never `tauri::async_runtime`. Evaluation happens inline on the
//! handler's worker (the lang layer builds `Rc`/`RefCell`, so it isn't `Send`);
//! only the resulting `Tracks` (which *is* `Send`) moves into the render thread.
//!
//! Cancellation is cooperative: the render core polls a closure between blocks, and
//! that closure round-trips `JobHandle::is_cancelled` to the shell registry (the
//! `__job_is_cancelled` reverse-channel arm) so the overlay Stop button stops the
//! bounce mid-flight.

use std::path::PathBuf;

use merula::prelude::{
    analyze_levels, export_midi, render_offline_with_progress, BitDepth, ControlMap, Format,
    RenderConfig, RenderOutcome, RenderProgress, Tracks,
};
use serde::Deserialize;

use merula_core::config::{self as config_cmds, MerulaConfig};
use merula_core::events::EVT_DIAGNOSTICS;
use crate::jobs::{category, JobHandle};
use merula_core::prelude::MerulaState;

/// Front-end render options. `cycles` is required (a `Pattern` has no intrinsic
/// length); format defaults come from `[render]` config unless overridden.
#[derive(Debug, Clone, Deserialize)]
pub struct RenderOpts {
    /// How many cycles of the arrangement to bounce.
    pub cycles: u32,
    /// First cycle of the bounce window (a region export). Defaults to `0` (the
    /// whole arrangement from the top) when absent.
    #[serde(default)]
    pub start_cycle: Option<u32>,
    /// `"int24"` | `"float32"` — overrides the config default when present.
    pub bit_depth: Option<String>,
    /// Trailing tail in seconds — overrides the config default when present.
    pub tail_max_secs: Option<f32>,
    /// Output sample rate — overrides the config default when present.
    pub sample_rate: Option<u32>,
    /// Output container/codec: `"wav"` | `"ogg"`. Defaults to WAV.
    pub format: Option<String>,
    /// Target integrated loudness (LUFS) to normalize to, or absent for no
    /// normalization (the default). A per-export choice, never persisted.
    #[serde(default)]
    pub normalize_lufs: Option<f32>,
}

/// Resolve a render config for a **batch export**, where the format applies to every
/// file (see `crate::export_all`). Same rules as [`resolve_config`], but the format is
/// mandatory rather than optional — a batch has one format by construction.
pub(crate) fn resolve_export_config(
    base: RenderConfig,
    opts: &crate::export_all::ExportAllOpts,
) -> RenderConfig {
    RenderConfig {
        sample_rate: opts.sample_rate.unwrap_or(base.sample_rate),
        bit_depth: match opts.bit_depth.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("float32") => BitDepth::Float32,
            Some(_) => BitDepth::Int24,
            None => base.bit_depth,
        },
        tail_max_secs: opts.tail_max_secs.unwrap_or(base.tail_max_secs),
        format: if opts.format.eq_ignore_ascii_case("ogg") {
            Format::Ogg
        } else {
            Format::Wav
        },
        normalize: opts.normalize_lufs.or(base.normalize),
    }
}

/// Resolve the effective [`RenderConfig`] by overlaying `opts` onto `base`.
fn resolve_config(base: RenderConfig, opts: &RenderOpts) -> RenderConfig {
    RenderConfig {
        sample_rate: opts.sample_rate.unwrap_or(base.sample_rate),
        bit_depth: match opts.bit_depth.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("float32") => BitDepth::Float32,
            Some(_) => BitDepth::Int24,
            None => base.bit_depth,
        },
        tail_max_secs: opts.tail_max_secs.unwrap_or(base.tail_max_secs),
        format: match opts.format.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("ogg") => Format::Ogg,
            Some(_) => Format::Wav,
            None => base.format,
        },
        normalize: opts.normalize_lufs.or(base.normalize),
    }
}

/// Evaluate `source` for a render/export command: on a language error, push the
/// diagnostics to the editor and return the first message as the command error
/// (so the export fails loudly rather than producing nothing). On success returns
/// the evaluated output. Shared by the WAV / stems / MIDI / analyze paths.
fn eval_for_render(
    ctx: &MerulaState,
    source: &str,
    project_dir: Option<String>,
    cfg: &MerulaConfig,
) -> Result<merula::prelude::EvalOutput, String> {
    let base = project_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    match crate::eval::evaluate_source(ctx.event_sink(), source, base, cfg.eval_config()) {
        Ok(o) => Ok(o),
        Err(diags) => {
            merula_core::events::emit(&*ctx.event_sink(), EVT_DIAGNOSTICS, &diags);
            let msg = diags
                .errors
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "evaluation failed".to_string());
            Err(msg)
        }
    }
}

/// Offline render runs at a constant tempo: the starting `tempo(...)` point if a
/// map is present (full offline tempo automation is a future refinement), else the
/// script's `cps(...)`, else the configured default. Centralised so the WAV /
/// stems / MIDI / analyze paths agree on the rendered tempo.
pub(crate) fn render_cps(output: &merula::prelude::EvalOutput, cfg: &MerulaConfig) -> f64 {
    output
        .tempo
        .points
        .first()
        .map(|p| p.1)
        .or(output.cps)
        .unwrap_or(cfg.default_cps)
}

/// Sanitize a track name into a safe filename stem. Keeps alphanumerics / dash /
/// underscore / space, collapses everything else to `_`; an empty result falls
/// back to `track`.
fn safe_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let s = s.trim();
    if s.is_empty() { "track".to_string() } else { s.to_string() }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Render `source` to a WAV/OGG file at `path` over `opts.cycles` cycles, on a
/// background job. Returns the job id. Evaluation errors fail the render (and are
/// emitted as diagnostics for the editor).
#[arbor_rpc::handler]
fn merula_render(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
    path: String,
    opts: RenderOpts,
) -> Result<String, String> {
    let cfg = config_cmds::load();
    let output = eval_for_render(ctx, &source, project_dir, &cfg)?;
    let cps = render_cps(&output, &cfg);
    let render_cfg = resolve_config(cfg.render.render_config(), &opts);
    spawn_render(
        ctx,
        output.tracks,
        cps,
        opts.start_cycle.unwrap_or(0),
        opts.cycles,
        render_cfg,
        PathBuf::from(path),
    )
}

/// Render `source` to **per-track stems** (one WAV/OGG per track) in `dir`, on a
/// background job. Returns the job id. Like [`merula_render`] but bounces each
/// track in isolation; evaluation errors fail the export.
#[arbor_rpc::handler]
fn merula_render_stems(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
    dir: String,
    opts: RenderOpts,
) -> Result<String, String> {
    let cfg = config_cmds::load();
    let output = eval_for_render(ctx, &source, project_dir, &cfg)?;
    let cps = render_cps(&output, &cfg);
    let render_cfg = resolve_config(cfg.render.render_config(), &opts);
    spawn_render_stems(
        ctx,
        output.tracks,
        cps,
        opts.start_cycle.unwrap_or(0),
        opts.cycles,
        render_cfg,
        PathBuf::from(dir),
    )
}

/// Export `source` to a Standard MIDI File at `path`, baking the arrangement's
/// natural loop period (one pass of the song). Unlike [`merula_render`] this is a
/// quick, note-only walk (no audio), so it resolves with the written summary
/// directly rather than running as a tracked job. Evaluation errors fail the
/// export (and are emitted as diagnostics for the editor).
#[arbor_rpc::handler]
fn merula_export_midi(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
    path: String,
) -> Result<MidiExportResult, String> {
    let cfg = config_cmds::load();
    let output = eval_for_render(ctx, &source, project_dir, &cfg)?;
    let cps = render_cps(&output, &cfg);
    // Bake the arrangement's detected loop period — the whole song, once.
    let cycles = crate::query::collect_haps(&output.tracks, crate::query::SNIPPET_WINDOW)
        .2
        .max(1);

    let summary = export_midi(&output.tracks, cps, cycles, &PathBuf::from(path))
        .map_err(|e| e.to_string())?;
    Ok(MidiExportResult { tracks: summary.tracks, notes: summary.notes })
}

/// Analyze `source` for clipping **without playing it**: a silent offline render
/// over the arrangement's loop period that measures each track's post-fader peak
/// and reports where it exceeds 0 dBFS. Quick (no file IO, no audio output). A bad
/// snippet resolves to an empty result (lints live elsewhere), so it's safe to
/// call live while editing.
#[arbor_rpc::handler]
fn merula_analyze_levels(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
) -> Result<LevelAnalysisResult, String> {
    let cfg = config_cmds::load();
    let base = project_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    // A bad snippet is not an error here (the editor lints elsewhere): resolve to
    // an empty analysis so the live clip indicator just goes quiet.
    let output = match crate::eval::evaluate_source(ctx.event_sink(), &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => {
            return Ok(LevelAnalysisResult { track_peaks: Vec::new(), clips: Vec::new() })
        }
    };

    let cps = render_cps(&output, &cfg);
    let cycles = crate::query::collect_haps(&output.tracks, crate::query::SNIPPET_WINDOW)
        .2
        .max(1);
    let sr = cfg.render.render_config().sample_rate;

    let analysis = analyze_levels(&output.tracks, cps, cycles, sr);
    Ok(LevelAnalysisResult {
        track_peaks: analysis.track_peaks,
        clips: analysis
            .clips
            .into_iter()
            .map(|c| ClipWindowDto {
                track: c.track,
                start: c.start_cycle,
                end: c.end_cycle,
                peak: c.peak,
            })
            .collect(),
    })
}

// ── Result DTOs ───────────────────────────────────────────────────────────────

/// Result of a MIDI export: how many MIDI tracks + notes were written. Surfaced to
/// the front end for the export notification.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct MidiExportResult {
    /// MIDI tracks written (one per merula track that produced ≥1 note).
    pub tracks: u32,
    /// Total notes written across all tracks.
    pub notes: u32,
}

/// One overload window from the offline level analysis: a track index plus the
/// cycle range whose post-fader level exceeds 0 dBFS.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ClipWindowDto {
    pub track: u32,
    /// Window start in cycles (absolute timeline).
    pub start: f64,
    /// Window end in cycles.
    pub end: f64,
    /// Deepest post-fader peak in the window, linear (`1.0` = 0 dBFS).
    pub peak: f32,
}

/// Result of an offline level analysis: per-track peak (linear, post-fader) and the
/// clip windows over the loop.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LevelAnalysisResult {
    pub track_peaks: Vec<f32>,
    pub clips: Vec<ClipWindowDto>,
}

// ── Background bounce workers ─────────────────────────────────────────────────

/// Spawn a background render job. Returns the job id immediately; the WAV/OGG is
/// written off-thread and completion is reported via the *Downloads & Exports*
/// overlay (`arbor://job-progress` + the terminal `job-done`).
fn spawn_render(
    ctx: &MerulaState,
    tracks: Tracks<ControlMap>,
    cps: f64,
    start_cycle: u32,
    cycles: u32,
    cfg: RenderConfig,
    out_path: PathBuf,
) -> Result<String, String> {
    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_render: no reverse channel".to_string())?;
    let name = format!("Render {}", out_path.display());
    let command = format!("render {cycles} cycles @ {cps} cps from cycle {start_cycle}");
    let job = JobHandle::register(host, ctx.event_sink(), &name, &command, category::RENDERS)?;
    let job_id = job.id.clone();

    // Plain OS thread: render_offline is blocking CPU/IO, not async. Never the audio
    // RT thread, never the dispatcher worker (a long bounce would block a request).
    let spawn = std::thread::Builder::new()
        .name(format!("merula-render-{job_id}"))
        .spawn(move || {
            // Forward render progress to the FE (throttled to whole-percent steps so
            // a long bounce emits ~100 events, not one per block).
            let progress_job = job.clone_handle();
            let mut last_pct: i32 = -1;
            let on_progress = move |p: RenderProgress| {
                let pct = (p.fraction() * 100.0).round() as i32;
                if pct != last_pct {
                    last_pct = pct;
                    progress_job.emit_progress(pct);
                }
            };
            // Cooperative cancellation: the render core polls this before each block,
            // so the overlay Stop button (→ `cancel_job` → `__job_is_cancelled`)
            // stops the bounce instead of running to completion.
            let cancel_job = job.clone_handle();
            let should_cancel = move || cancel_job.is_cancelled();
            // Catch a panic in the render core so the job reports Failed (with a
            // surfaced message) instead of hanging on Running and leaving an
            // unfinalized, unplayable file with no explanation.
            let outcome: Result<RenderOutcome, String> = match std::panic::catch_unwind(
                std::panic::AssertUnwindSafe(|| {
                    render_offline_with_progress(
                        &tracks, cps, start_cycle, cycles, &cfg, &out_path, on_progress,
                        should_cancel,
                    )
                }),
            ) {
                Ok(Ok(o)) => Ok(o),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("render thread panicked (see the log for details)".to_string()),
            };
            match outcome {
                Ok(RenderOutcome::Completed) => job.finish_ok(),
                // Cancelled: drop the partial file (the user stopped it — a stray
                // half-render is just clutter) and report a cancel, not a failure.
                Ok(RenderOutcome::Cancelled) => {
                    let _ = std::fs::remove_file(&out_path);
                    job.finish_cancelled();
                }
                Err(msg) => {
                    job.append(&msg);
                    job.finish_failed(msg);
                }
            }
        });
    if let Err(e) = spawn {
        // The handle moved into the closure that failed to start, so the registry
        // entry is left Running; the spawn failure is the command error the caller
        // surfaces (the FE never received a job id to track).
        return Err(format!("failed to spawn render thread: {e}"));
    }
    Ok(job_id)
}

/// Spawn a background **stems** export: bounce each track to its own WAV/OGG in
/// `out_dir` (`NN_name.ext`), in non-real-time. One tracked job covers the whole
/// set, with progress spanning all stems; cancellation stops between/within stems
/// (already-written stems are left in place — they're valid files). Each stem
/// renders the track in isolation (a one-track `Tracks`), so its baked gain/pan/EX
/// inserts carry over but other tracks don't bleed in.
fn spawn_render_stems(
    ctx: &MerulaState,
    tracks: Tracks<ControlMap>,
    cps: f64,
    start_cycle: u32,
    cycles: u32,
    cfg: RenderConfig,
    out_dir: PathBuf,
) -> Result<String, String> {
    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_render_stems: no reverse channel".to_string())?;
    let count = tracks.tracks.len();
    let name = format!("Stems {}", out_dir.display());
    let command =
        format!("stems {count} tracks · {cycles} cycles @ {cps} cps from cycle {start_cycle}");
    let job = JobHandle::register(host, ctx.event_sink(), &name, &command, category::RENDERS)?;
    let job_id = job.id.clone();
    let ext = cfg.format.extension();

    let spawn = std::thread::Builder::new()
        .name(format!("merula-stems-{job_id}"))
        .spawn(move || {
            let total = count.max(1);
            // Best-effort: a real write error surfaces from the first stem render.
            let _ = std::fs::create_dir_all(&out_dir);

            // Overall percent across all stems (stem `idx` contributes its own 0..1
            // fraction within its 1/total slice). Throttled to whole steps.
            let last_pct = std::cell::Cell::new(-1i32);
            let mut outcome: Result<RenderOutcome, String> = Ok(RenderOutcome::Completed);

            for (idx, track) in tracks.tracks.iter().enumerate() {
                if job.is_cancelled() {
                    outcome = Ok(RenderOutcome::Cancelled);
                    break;
                }
                let stem = Tracks { tracks: vec![track.clone()] };
                let file = out_dir.join(format!("{:02}_{}.{ext}", idx + 1, safe_stem(&track.name)));

                // Per-stem progress closure; `last` is a shared ref into the
                // loop-scoped counter so the overall percent persists across stems.
                let progress_job = job.clone_handle();
                let last = &last_pct;
                let on_progress = move |p: RenderProgress| {
                    let overall =
                        ((idx as f32 + p.fraction()) / total as f32 * 100.0).round() as i32;
                    if overall != last.get() {
                        last.set(overall);
                        progress_job.emit_progress(overall);
                    }
                };
                let cancel_job = job.clone_handle();
                let should_cancel = move || cancel_job.is_cancelled();

                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    render_offline_with_progress(
                        &stem, cps, start_cycle, cycles, &cfg, &file, on_progress, should_cancel,
                    )
                }));
                match res {
                    Ok(Ok(RenderOutcome::Completed)) => {}
                    Ok(Ok(RenderOutcome::Cancelled)) => {
                        outcome = Ok(RenderOutcome::Cancelled);
                        break;
                    }
                    Ok(Err(e)) => {
                        outcome = Err(e.to_string());
                        break;
                    }
                    Err(_) => {
                        outcome =
                            Err("stems render thread panicked (see the log for details)".to_string());
                        break;
                    }
                }
            }

            match outcome {
                Ok(RenderOutcome::Completed) => job.finish_ok(),
                // Cancelled: keep the stems already written (each is a valid file)
                // and report a cancel, not a failure.
                Ok(RenderOutcome::Cancelled) => job.finish_cancelled(),
                Err(msg) => {
                    job.append(&msg);
                    job.finish_failed(msg);
                }
            }
        });
    if let Err(e) = spawn {
        return Err(format!("failed to spawn stems thread: {e}"));
    }
    Ok(job_id)
}
