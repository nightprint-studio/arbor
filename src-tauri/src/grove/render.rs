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

use arbor_grove::prelude::{render_offline, ControlMap, RenderConfig, Tracks};

use crate::jobs::{JobInfo, JobRegistry, JobStatus};

/// Front-end render options. `cycles` is required (a `Pattern` has no intrinsic
/// length); format defaults come from `[grove].render` config unless overridden.
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
    use arbor_grove::prelude::BitDepth;
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
    let job_id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return String::new(),
        };
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: format!("Render {}", out_path.display()),
            plugin_name: "grove".to_string(),
            command: format!("render {cycles} cycles @ {cps} cps"),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Renders".to_string()),
            non_cancellable: false,
            hidden: false,
            is_system: false,
            finished_at: None,
        });
        id
    };

    let app = app.clone();
    let job_id_thread = job_id.clone();
    // Plain OS thread: render_offline is blocking CPU/IO, not async.
    if let Err(e) = std::thread::Builder::new()
        .name(format!("grove-render-{job_id}"))
        .spawn(move || {
            let result = render_offline(&tracks, cps, cycles, &cfg, &out_path);
            let state = app.state::<crate::AppState>();
            let (status, success, error) = match result {
                Ok(()) => (JobStatus::Completed { exit_code: 0 }, true, None),
                Err(e) => {
                    let msg = e.to_string();
                    (JobStatus::Failed { error: msg.clone() }, false, Some(msg))
                }
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
        tracing::error!("grove: failed to spawn render thread: {e}");
    }

    job_id
}
