//! Offline render job: bounce `Tracks` to a WAV file in non-real-time.
//!
//! `render_offline` is synchronous CPU/IO work (its own block loop, not the
//! cpal callback), so it runs on a plain background thread tracked in the
//! [`JobRegistry`](crate::jobs::JobRegistry) — never the audio RT thread, never
//! the async runtime. Evaluation happens on the command thread (it isn't `Send`);
//! only the resulting `Tracks` (which is `Send`) moves into the render thread.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use arbor_nemus::prelude::{render_offline_with_progress, ControlMap, RenderConfig, RenderOutcome, Tracks};

use crate::jobs::{JobInfo, JobRegistry, JobStatus};

/// Front-end render options. `cycles` is required (a `Pattern` has no intrinsic
/// length); format defaults come from `[nemus].render` config unless overridden.
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

/// True once the user cancelled the render job (the Transfers overlay Stop
/// button → `cancel_job` → `JobStatus::Cancelled` in the registry). Polled by
/// the render core's cooperative cancellation between blocks.
fn job_is_cancelled(app: &AppHandle, job_id: &str) -> bool {
    app.state::<crate::AppState>()
        .jobs
        .lock()
        .map(|j| j.is_cancelled(job_id))
        .unwrap_or(false)
}

/// Resolve the effective [`RenderConfig`] by overlaying `opts` onto `base`.
pub fn resolve_config(base: RenderConfig, opts: &RenderOpts) -> RenderConfig {
    use arbor_nemus::prelude::{BitDepth, Format};
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

/// Register a hidden, cancellable nemus render/stems job (routed to the nemus
/// feedback host) and emit its `job-started`. Returns the job id, or `""` if the
/// registry lock is poisoned (the caller bails). Shared by the single-WAV bounce
/// and the per-track stems export so they register identically.
fn register_render_job(app: &AppHandle, name: &str, command: &str) -> String {
    let state = app.state::<crate::AppState>();
    let job_id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return String::new(),
        };
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: name.to_string(),
            plugin_name: "nemus".to_string(),
            command: command.to_string(),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Renders".to_string()),
            non_cancellable: false,
            // Hidden from the Jobs panel / overlay / badge: the render is the
            // user-facing surface of the nemus **Downloads & Exports** overlay
            // (it streams `job-progress` + a revealable output path there), so
            // showing it in Jobs too would be a duplicate. Still registered
            // (tracked, revealable via "Show hidden") to keep the job-event
            // invariants intact.
            hidden: true,
            is_system: false,
            finished_at: None,
            // Route to the nemus window's feedback host (it mounts
            // <FeedbackHost id="nemus">), so renders surface there, not in main.
            target: Some("nemus".to_string()),
        });
        id
    };
    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        name,
        "plugin_name": "nemus",
        "command":     command,
        "category":    "Renders",
        "hidden":      true,
        "target":      "nemus",
    }));
    job_id
}

/// Sanitize a track name into a safe filename stem (`NN_name.ext`). Keeps
/// alphanumerics / dash / underscore / space, collapses everything else to `_`;
/// an empty result falls back to `track`.
fn safe_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let s = s.trim();
    if s.is_empty() { "track".to_string() } else { s.to_string() }
}

/// Spawn a background render job. Returns the job id immediately; the WAV is
/// written off-thread and completion is reported via the Jobs overlay.
pub fn spawn_render(
    app: &AppHandle,
    tracks: Tracks<ControlMap>,
    cps: f64,
    start_cycle: u32,
    cycles: u32,
    cfg: RenderConfig,
    out_path: PathBuf,
) -> String {
    let name = format!("Render {}", out_path.display());
    let command = format!("render {cycles} cycles @ {cps} cps from cycle {start_cycle}");
    let job_id = register_render_job(app, &name, &command);
    if job_id.is_empty() {
        return job_id;
    }

    let app = app.clone();
    let job_id_thread = job_id.clone();
    // Plain OS thread: render_offline is blocking CPU/IO, not async.
    if let Err(e) = std::thread::Builder::new()
        .name(format!("nemus-render-{job_id}"))
        .spawn(move || {
            // Forward render progress to the FE (throttled to whole-percent steps
            // so a long bounce emits ~100 events, not one per block).
            let progress_app = app.clone();
            let progress_job = job_id_thread.clone();
            let mut last_pct: i32 = -1;
            let on_progress = move |p: arbor_nemus::prelude::RenderProgress| {
                let pct = (p.fraction() * 100.0).round() as i32;
                if pct != last_pct {
                    last_pct = pct;
                    let _ = progress_app.emit(
                        "arbor://job-progress",
                        serde_json::json!({ "job_id": progress_job, "pct": pct }),
                    );
                }
            };
            // Cooperative cancellation: the render core polls this before each
            // block, so `cancel_job` (the Transfers overlay Stop button) stops the
            // bounce instead of running to completion. Locking the (uncontended)
            // job registry once per block is cheap and off the RT path.
            let cancel_app = app.clone();
            let cancel_job_id = job_id_thread.clone();
            let should_cancel = move || job_is_cancelled(&cancel_app, &cancel_job_id);
            // Catch a panic in the render core so the job reports Failed (with a
            // surfaced message) instead of hanging on Running and leaving an
            // unfinalized, unplayable WAV with no explanation.
            let outcome: Result<RenderOutcome, String> = match std::panic::catch_unwind(
                std::panic::AssertUnwindSafe(|| {
                    render_offline_with_progress(&tracks, cps, start_cycle, cycles, &cfg, &out_path, on_progress, should_cancel)
                }),
            ) {
                Ok(Ok(o)) => Ok(o),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("render thread panicked (see the log for details)".to_string()),
            };
            let state = app.state::<crate::AppState>();
            let (status, success, error) = match outcome {
                Ok(RenderOutcome::Completed) => (JobStatus::Completed { exit_code: 0 }, true, None),
                // Cancelled: drop the partial WAV (the user stopped it — a stray
                // half-render is just clutter) and report it as a cancel, not a
                // failure, so the overlay shows "Cancelled".
                Ok(RenderOutcome::Cancelled) => {
                    let _ = std::fs::remove_file(&out_path);
                    (JobStatus::Cancelled, false, None)
                }
                Err(msg) => (JobStatus::Failed { error: msg.clone() }, false, Some(msg)),
            };
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.set_status(&job_id_thread, status);
            }
            let _ = app.emit(
                "arbor://job-done",
                serde_json::json!({ "job_id": job_id_thread, "success": success, "error": error }),
            );
        })
    {
        tracing::error!("nemus: failed to spawn render thread: {e}");
    }

    job_id
}

/// Spawn a background **stems** export: bounce each track to its own WAV/OGG in
/// `out_dir` (`NN_name.ext`), in non-real-time. One tracked job covers the whole
/// set, with progress spanning all stems; cancellation stops between/within
/// stems (already-written stems are left in place — they're valid files). Each
/// stem renders the track in isolation (a one-track `Tracks`), so its baked
/// gain/pan/EX inserts carry over but other tracks don't bleed in.
pub fn spawn_render_stems(
    app: &AppHandle,
    tracks: Tracks<ControlMap>,
    cps: f64,
    start_cycle: u32,
    cycles: u32,
    cfg: RenderConfig,
    out_dir: PathBuf,
) -> String {
    let count = tracks.tracks.len();
    let name = format!("Stems {}", out_dir.display());
    let command = format!("stems {count} tracks · {cycles} cycles @ {cps} cps from cycle {start_cycle}");
    let job_id = register_render_job(app, &name, &command);
    if job_id.is_empty() {
        return job_id;
    }

    let ext = cfg.format.extension();
    let app = app.clone();
    let job_id_thread = job_id.clone();
    // Plain OS thread: each stem render is blocking CPU/IO, not async.
    if let Err(e) = std::thread::Builder::new()
        .name(format!("nemus-stems-{job_id}"))
        .spawn(move || {
            let total = count.max(1);
            // Best-effort: a real write error surfaces from the first stem render.
            let _ = std::fs::create_dir_all(&out_dir);

            // Overall percent across all stems (stem `idx` contributes its own
            // 0..1 fraction within its 1/total slice). Throttled to whole steps.
            let last_pct = std::cell::Cell::new(-1i32);
            let mut outcome: Result<RenderOutcome, String> = Ok(RenderOutcome::Completed);

            for (idx, track) in tracks.tracks.iter().enumerate() {
                if job_is_cancelled(&app, &job_id_thread) {
                    outcome = Ok(RenderOutcome::Cancelled);
                    break;
                }
                let stem = Tracks { tracks: vec![track.clone()] };
                let file = out_dir.join(format!("{:02}_{}.{ext}", idx + 1, safe_stem(&track.name)));

                // Per-stem closures own clones of the handle + job id (mirroring
                // the single-render path); `last` is a shared ref into the
                // loop-scoped counter so the overall percent persists across stems.
                let p_app = app.clone();
                let p_job = job_id_thread.clone();
                let last = &last_pct;
                let on_progress = move |p: arbor_nemus::prelude::RenderProgress| {
                    let overall = ((idx as f32 + p.fraction()) / total as f32 * 100.0).round() as i32;
                    if overall != last.get() {
                        last.set(overall);
                        let _ = p_app.emit(
                            "arbor://job-progress",
                            serde_json::json!({ "job_id": p_job, "pct": overall }),
                        );
                    }
                };
                let c_app = app.clone();
                let c_job = job_id_thread.clone();
                let should_cancel = move || job_is_cancelled(&c_app, &c_job);

                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    render_offline_with_progress(&stem, cps, start_cycle, cycles, &cfg, &file, on_progress, should_cancel)
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
                        outcome = Err("stems render thread panicked (see the log for details)".to_string());
                        break;
                    }
                }
            }

            let state = app.state::<crate::AppState>();
            let (status, success, error) = match outcome {
                Ok(RenderOutcome::Completed) => (JobStatus::Completed { exit_code: 0 }, true, None),
                // Cancelled: keep the stems already written (each is a valid file)
                // and report a cancel, not a failure.
                Ok(RenderOutcome::Cancelled) => (JobStatus::Cancelled, false, None),
                Err(msg) => (JobStatus::Failed { error: msg.clone() }, false, Some(msg)),
            };
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.set_status(&job_id_thread, status);
            }
            let _ = app.emit(
                "arbor://job-done",
                serde_json::json!({ "job_id": job_id_thread, "success": success, "error": error }),
            );
        })
    {
        tracing::error!("nemus: failed to spawn stems thread: {e}");
    }

    job_id
}
