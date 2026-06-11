//! grove backend shell: the runtime that ties the grove crates
//! (`pattern`/`lang`/`audio`/`engine`, behind the `arbor-grove` facade) to the
//! Arbor app.
//!
//! Responsibilities (Onda 3, backend only — no Svelte):
//! - own the per-window audio session: a dedicated thread holding the cpal stream
//!   + the `Transport` look-ahead driver ([`audio_thread`]). Started lazily on
//!   first **play**, torn down on window close. Never the job system, never the
//!   async runtime — the real-time path is sacred.
//! - orchestrate re-eval: `lang(source)` → `Tracks` → `Transport::set_tracks`
//!   (quantized at the next cycle boundary), with span-located diagnostics
//!   ([`eval`]).
//! - manage the VSCO 2 sample bank: download/extract/index via the job system,
//!   wire the resulting sound registry into the audio session ([`vsco`]).
//! - offline render to WAV via a background job ([`render`]).
//! - push throttled BE→FE events (`grove:diagnostics`/`active_haps`/`meters`/
//!   `transport`/`log`, [`events`]).

mod audio_thread;
pub mod config;
mod control;
mod eval;
mod events;
mod render;
mod vsco;

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use tauri::{AppHandle, Manager, State};

use arbor_grove::prelude::{ControlMap, Tracks};

use crate::config::app_config;
use crate::error::AppError;
use crate::AppState;

pub use config::GroveConfig;
use control::GroveControl;
use events::{emit, GroveDiagnostics, EVT_DIAGNOSTICS};
use render::RenderOpts;
use vsco::VscoStatus;

/// Per-window grove runtime state, managed in Tauri.
#[derive(Default)]
pub struct GroveState {
    /// The live audio session (`None` until first play / after shutdown).
    session: Mutex<Option<Session>>,
    /// The most recent good evaluation, replayed when a session starts.
    latest: Mutex<Option<Latest>>,
}

/// A running audio session.
struct Session {
    tx: Sender<GroveControl>,
    handle: JoinHandle<()>,
}

/// The last successfully evaluated arrangement.
struct Latest {
    tracks: Tracks<ControlMap>,
    cps: Option<f64>,
}

impl GroveState {
    /// Return a sender to the audio thread, starting it if needed. Opening the
    /// session opens the audio device — done lazily here (on play), not on eval.
    fn ensure_session(&self, app: &AppHandle, cfg: &GroveConfig) -> Sender<GroveControl> {
        let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = guard.as_ref() {
            if !s.handle.is_finished() {
                return s.tx.clone();
            }
        }
        let (tx, rx) = mpsc::channel();
        let app2 = app.clone();
        let cfg2 = cfg.clone();
        let handle = std::thread::Builder::new()
            .name("grove-audio".to_string())
            .spawn(move || audio_thread::run(app2, rx, cfg2))
            .expect("spawn grove-audio thread");
        *guard = Some(Session {
            tx: tx.clone(),
            handle,
        });
        tx
    }

    /// Send to the live session, if any. No-op when nothing is running.
    fn send_if_live(&self, msg: GroveControl) {
        let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = guard.as_ref() {
            let _ = s.tx.send(msg);
        }
    }

    /// Tear the session down (drop the cpal stream on its thread) and join.
    /// Called on grove-window close.
    pub fn shutdown(&self) {
        let session = {
            let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(s) = session {
            let _ = s.tx.send(GroveControl::Shutdown);
            let _ = s.handle.join();
        }
    }
}

/// Read the grove config from app state.
fn grove_config(state: &State<'_, AppState>) -> Result<GroveConfig, AppError> {
    Ok(state.lock_config()?.grove.clone())
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Evaluate `.grove` source and stage it as the live arrangement. Returns
/// diagnostics (errors with span); language errors are diagnostics, not command
/// failures, so this still returns `Ok`. Does **not** open the audio device —
/// that happens on the first `play` (the staged result is replayed then).
#[tauri::command]
pub async fn grove_eval(
    app: AppHandle,
    state: State<'_, AppState>,
    grove: State<'_, GroveState>,
    source: String,
    project_dir: Option<String>,
) -> Result<GroveDiagnostics, AppError> {
    let cfg = grove_config(&state)?;
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let diagnostics = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(output) => {
            // Stash for replay on the next play, and push live if already running.
            {
                let mut latest = grove.latest.lock().unwrap_or_else(|e| e.into_inner());
                *latest = Some(Latest {
                    tracks: output.tracks.clone(),
                    cps: output.cps,
                });
            }
            grove.send_if_live(GroveControl::SetTracks {
                tracks: output.tracks,
                cps: output.cps,
            });
            GroveDiagnostics::ok()
        }
        Err(diags) => diags,
    };

    emit(&app, EVT_DIAGNOSTICS, diagnostics.clone());
    Ok(diagnostics)
}

/// Transport control. `action` ∈ `play` | `stop` | `seek` | `set_cps`; `value`
/// carries the target cycle (`seek`) or tempo (`set_cps`).
#[tauri::command]
pub async fn grove_transport(
    app: AppHandle,
    state: State<'_, AppState>,
    grove: State<'_, GroveState>,
    action: String,
    value: Option<f64>,
) -> Result<(), AppError> {
    match action.as_str() {
        "play" => {
            let cfg = grove_config(&state)?;
            let tx = grove.ensure_session(&app, &cfg);
            // Feed the freshly-started transport the latest arrangement before
            // starting it (harmless if a live eval already pushed the same).
            {
                let latest = grove.latest.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(l) = latest.as_ref() {
                    let _ = tx.send(GroveControl::SetTracks {
                        tracks: l.tracks.clone(),
                        cps: l.cps,
                    });
                }
            }
            let _ = tx.send(GroveControl::Play);
        }
        "stop" => grove.send_if_live(GroveControl::Stop),
        "seek" => {
            grove.send_if_live(GroveControl::Seek {
                cycle: value.unwrap_or(0.0),
            });
        }
        "set_cps" => {
            if let Some(cps) = value {
                grove.send_if_live(GroveControl::SetCps { cps });
            }
        }
        other => return Err(AppError::Unsupported(format!("grove transport: {other}"))),
    }
    Ok(())
}

/// Render `source` to a WAV file at `path` over `opts.cycles` cycles, on a
/// background job. Returns the job id. Evaluation errors fail the render (and are
/// emitted as diagnostics for the editor).
#[tauri::command]
pub async fn grove_render(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
    project_dir: Option<String>,
    path: String,
    opts: RenderOpts,
) -> Result<String, AppError> {
    let cfg = grove_config(&state)?;
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let output = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(diags) => {
            emit(&app, EVT_DIAGNOSTICS, diags.clone());
            let msg = diags
                .errors
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "evaluation failed".to_string());
            return Err(AppError::Grove(msg));
        }
    };

    let cps = output.cps.unwrap_or(cfg.default_cps);
    let render_cfg = render::resolve_config(cfg.render.render_config(), &opts);
    let job_id = render::spawn_render(
        &app,
        output.tracks,
        cps,
        opts.cycles,
        render_cfg,
        std::path::PathBuf::from(path),
    );
    Ok(job_id)
}

/// Read the VSCO 2 sample-bank install status.
#[tauri::command]
pub async fn grove_vsco_status(
    state: State<'_, AppState>,
) -> Result<VscoStatus, AppError> {
    let cfg = grove_config(&state)?;
    Ok(vsco::status(&cfg))
}

/// Start downloading + installing the VSCO 2 sample bank (job-tracked). Returns
/// the job id; cancel via the standard `cancel_job`.
#[tauri::command]
pub async fn grove_vsco_download(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let cfg = grove_config(&state)?;
    Ok(vsco::start_download(&app, &cfg))
}

/// Read the grove config (`[grove]` in the global config.toml).
#[tauri::command]
pub fn get_grove_config(state: State<'_, AppState>) -> Result<GroveConfig, AppError> {
    grove_config(&state)
}

/// Persist a new grove config. Takes effect for the next session / render.
#[tauri::command]
pub fn set_grove_config(
    state: State<'_, AppState>,
    grove: GroveConfig,
) -> Result<(), AppError> {
    let mut config = state.lock_config()?;
    config.grove = grove;
    app_config::save(&config).map_err(|e| AppError::Other(e.to_string()))
}

/// Tear down the grove audio session for the app (window close). Safe to call
/// when nothing is running.
pub fn shutdown(app: &AppHandle) {
    if let Some(grove) = app.try_state::<GroveState>() {
        grove.shutdown();
    }
}
