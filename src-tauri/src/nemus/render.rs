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

use arbor_nemus::prelude::{render_offline_with_progress, ControlMap, RenderConfig, Tracks};

use crate::jobs::{JobInfo, JobRegistry, JobStatus};

/// Front-end render options. `cycles` is required (a `Pattern` has no intrinsic
/// length); format defaults come from `[nemus].render` config unless overridden.
#[derive(Debug, Clone, Deserialize)]
pub struct RenderOpts {
    /// How many cycles of the arrangement to bounce.
    pub cycles: u32,
    /// `"int24"` | `"float32"` — overrides the config default when present.
    pub bit_depth: Option<String>,
    /// Trailing tail in seconds — overrides the config default when present.
    pub tail_max_secs: Option<f32>,
    /// Output sample rate — overrides the config default when present.
    pub sample_rate: Option<u32>,
}

/// Resolve the effective [`RenderConfig`] by overlaying `opts` onto `base`.
pub fn resolve_config(base: RenderConfig, opts: &RenderOpts) -> RenderConfig {
    use arbor_nemus::prelude::BitDepth;
    RenderConfig {
        sample_rate: opts.sample_rate.unwrap_or(base.sample_rate),
        bit_depth: match opts.bit_depth.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("float32") => BitDepth::Float32,
            Some(_) => BitDepth::Int24,
            None => base.bit_depth,
        },
        tail_max_secs: opts.tail_max_secs.unwrap_or(base.tail_max_secs),
    }
}

/// Spawn a background render job. Returns the job id immediately; the WAV is
/// written off-thread and completion is reported via the Jobs overlay.
pub fn spawn_render(
    app: &AppHandle,
    tracks: Tracks<ControlMap>,
    cps: f64,
    cycles: u32,
    cfg: RenderConfig,
    out_path: PathBuf,
) -> String {
    let state = app.state::<crate::AppState>();
    let name = format!("Render {}", out_path.display());
    let command = format!("render {cycles} cycles @ {cps} cps");
    let job_id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return String::new(),
        };
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: name.clone(),
            plugin_name: "nemus".to_string(),
            command: command.clone(),
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
    // Register the job (hidden) so its terminal event has a registry entry; the
    // visible surface is the Transfers overlay, fed by job-progress / job-done.
    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &name,
        "plugin_name": "nemus",
        "command":     &command,
        "category":    "Renders",
        "hidden":      true,
        "target":      "nemus",
    }));

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
            // Catch a panic in the render core so the job reports Failed (with a
            // surfaced message) instead of hanging on Running and leaving an
            // unfinalized, unplayable WAV with no explanation.
            let outcome: Result<(), String> = match std::panic::catch_unwind(
                std::panic::AssertUnwindSafe(|| {
                    render_offline_with_progress(&tracks, cps, cycles, &cfg, &out_path, on_progress)
                }),
            ) {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("render thread panicked (see the log for details)".to_string()),
            };
            let state = app.state::<crate::AppState>();
            let (status, success, error) = match outcome {
                Ok(()) => (JobStatus::Completed { exit_code: 0 }, true, None),
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
