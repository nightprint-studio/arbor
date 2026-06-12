//! nemus backend shell: the runtime that ties the nemus crates
//! (`pattern`/`lang`/`audio`/`engine`, behind the `arbor-nemus` facade) to the
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
//! - manage downloadable sample packs (VSCO 2, Dirt-Samples, drum machines, …):
//!   download/extract/index via the job system, merge their sound registries
//!   into the audio session ([`packs`]).
//! - offline render to WAV via a background job ([`render`]).
//! - push throttled BE→FE events (`nemus:diagnostics`/`active_haps`/`meters`/
//!   `transport`/`log`, [`events`]).

mod audio_thread;
pub mod config;
mod control;
mod eval;
mod events;
pub mod import;
pub mod models;
mod packs;
pub mod project;
pub mod query;
pub mod reference;
mod render;
mod sound_catalog;
pub mod sounds;
pub mod state;
mod validate;

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use tauri::{AppHandle, Manager, State};

use arbor_nemus::prelude::{ControlMap, TempoMap, Tracks};

use crate::error::AppError;

pub use config::NemusConfig;
use control::NemusControl;
use events::{emit, NemusDiagnostics, EVT_DIAGNOSTICS};
use render::RenderOpts;

// The additive Fase-4 commands live in their own modules (`query`/`sounds`/
// `state`/`project`, all `pub`). The invoke handler references them by full path
// (`nemus::query::nemus_query`, …) like the rest of the app's submodule commands
// (e.g. `commands::repo_commands::open_repo`) — a `#[tauri::command]` generates
// helper macros next to the fn, so a bare `pub use` of the fn would not surface
// them. `nemus_set_track` stays defined inline below (referenced as
// `nemus::nemus_set_track`).

/// Per-window nemus runtime state, managed in Tauri.
#[derive(Default)]
pub struct NemusState {
    /// The live audio session (`None` until first play / after shutdown).
    session: Mutex<Option<Session>>,
    /// The most recent good evaluation, replayed when a session starts.
    latest: Mutex<Option<Latest>>,
}

/// A running audio session.
struct Session {
    tx: Sender<NemusControl>,
    handle: JoinHandle<()>,
}

/// The last successfully evaluated arrangement.
struct Latest {
    tracks: Tracks<ControlMap>,
    cps: Option<f64>,
    tempo: TempoMap,
}

impl NemusState {
    /// Return a sender to the audio thread, starting it if needed. Opening the
    /// session opens the audio device — done lazily here (on play), not on eval.
    fn ensure_session(&self, app: &AppHandle, cfg: &NemusConfig) -> Sender<NemusControl> {
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
            .name("nemus-audio".to_string())
            .spawn(move || audio_thread::run(app2, rx, cfg2))
            .expect("spawn nemus-audio thread");
        *guard = Some(Session {
            tx: tx.clone(),
            handle,
        });
        tx
    }

    /// Send to the live session, if any. No-op when nothing is running.
    fn send_if_live(&self, msg: NemusControl) {
        let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = guard.as_ref() {
            let _ = s.tx.send(msg);
        }
    }

    /// Tear the session down (drop the cpal stream on its thread) and join.
    /// Called on nemus-window close.
    pub fn shutdown(&self) {
        let session = {
            let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(s) = session {
            let _ = s.tx.send(NemusControl::Shutdown);
            let _ = s.handle.join();
        }
    }
}

/// Read nemus's config from its own `%APPDATA%\nemus\config.toml` (defaults on a
/// missing / corrupt file; never errors).
fn nemus_config() -> NemusConfig {
    config::load()
}

/// One-time storage migration, run once at app startup. Moves nemus's data out
/// of the old `<arbor-data>/nemus` location into its own `<nemus-data>` root and
/// seeds nemus's config file from Arbor's legacy `[nemus]` section. Cheap no-op
/// once migrated.
pub fn migrate_storage() {
    // Data: move `%APPDATA%\arbor\nemus` → `%APPDATA%\nemus` (a same-volume
    // rename, so the multi-GB sample banks aren't re-downloaded). Only when the
    // old tree exists and the new one doesn't, so it runs exactly once.
    let old = arbor_core::prelude::arbor_data_dir().join("nemus");
    let new = arbor_core::prelude::nemus_data_dir();
    if old.exists() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::rename(&old, &new) {
            tracing::warn!("nemus: storage migration {old:?} → {new:?} failed: {e}");
        }
    }
    // Config: seed nemus's own file from Arbor's legacy `[nemus]` section.
    config::migrate_if_needed();
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Evaluate `.nemus` source and stage it as the live arrangement. Returns
/// diagnostics (errors with span); language errors are diagnostics, not command
/// failures, so this still returns `Ok`. Does **not** open the audio device —
/// that happens on the first `play` (the staged result is replayed then).
#[tauri::command]
pub async fn nemus_eval(
    app: AppHandle,
    nemus: State<'_, NemusState>,
    source: String,
    project_dir: Option<String>,
) -> Result<NemusDiagnostics, AppError> {
    let cfg = nemus_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let diagnostics = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(output) => {
            // Surface sound/instrument references the registry can't resolve as
            // editor errors (the renderer would silently fall back to the synth).
            // Done before the arrangement is moved to the live session below.
            let known = validate::known_instruments(&cfg);
            let errors = validate::validate_instruments(&output.tracks, &known);
            // Stash for replay on the next play, and push live if already running.
            {
                let mut latest = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
                *latest = Some(Latest {
                    tracks: output.tracks.clone(),
                    cps: output.cps,
                    tempo: output.tempo.clone(),
                });
            }
            nemus.send_if_live(NemusControl::SetTracks {
                tracks: output.tracks,
                cps: output.cps,
                tempo: output.tempo,
            });
            NemusDiagnostics { errors }
        }
        Err(diags) => diags,
    };

    emit(&app, EVT_DIAGNOSTICS, diagnostics.clone());
    Ok(diagnostics)
}

/// Transport control. `action` ∈ `play` | `stop` | `seek` | `set_cps`; `value`
/// carries the target cycle (`seek`) or tempo (`set_cps`).
#[tauri::command]
pub async fn nemus_transport(
    app: AppHandle,
    nemus: State<'_, NemusState>,
    action: String,
    value: Option<f64>,
) -> Result<(), AppError> {
    match action.as_str() {
        "play" => {
            let cfg = nemus_config();
            let tx = nemus.ensure_session(&app, &cfg);
            // Feed the freshly-started transport the latest arrangement before
            // starting it (harmless if a live eval already pushed the same).
            {
                let latest = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(l) = latest.as_ref() {
                    let _ = tx.send(NemusControl::SetTracks {
                        tracks: l.tracks.clone(),
                        cps: l.cps,
                        tempo: l.tempo.clone(),
                    });
                }
            }
            let _ = tx.send(NemusControl::Play);
        }
        "stop" => nemus.send_if_live(NemusControl::Stop),
        "seek" => {
            nemus.send_if_live(NemusControl::Seek {
                cycle: value.unwrap_or(0.0),
            });
        }
        "set_cps" => {
            if let Some(cps) = value {
                nemus.send_if_live(NemusControl::SetCps { cps });
            }
        }
        other => return Err(AppError::Unsupported(format!("nemus transport: {other}"))),
    }
    Ok(())
}

/// Push a **live mixer override** to the running session (no-op when stopped).
/// `action` ∈ `gain` | `pan` | `mute` | `solo` | `master_gain`. These are
/// ephemeral session tweaks: the next `nemus_eval` re-baselines the mixer from
/// the source. `value` is `0..1` for gain/pan/master_gain, `0|1` for mute/solo;
/// `track` is ignored for `master_gain`.
#[tauri::command]
pub async fn nemus_set_track(
    nemus: State<'_, NemusState>,
    action: String,
    track: Option<u32>,
    value: f64,
) -> Result<(), AppError> {
    let msg = match action.as_str() {
        "gain" => NemusControl::SetTrackGain {
            track: track.unwrap_or(0),
            gain: value as f32,
        },
        "pan" => NemusControl::SetTrackPan {
            track: track.unwrap_or(0),
            pan: value as f32,
        },
        "mute" => NemusControl::SetTrackMute {
            track: track.unwrap_or(0),
            mute: value != 0.0,
        },
        "solo" => NemusControl::SetTrackSolo {
            track: track.unwrap_or(0),
            solo: value != 0.0,
        },
        "master_gain" => NemusControl::SetMasterGain { gain: value as f32 },
        other => return Err(AppError::Unsupported(format!("nemus set_track: {other}"))),
    };
    nemus.send_if_live(msg);
    Ok(())
}

/// Render `source` to a WAV file at `path` over `opts.cycles` cycles, on a
/// background job. Returns the job id. Evaluation errors fail the render (and are
/// emitted as diagnostics for the editor).
#[tauri::command]
pub async fn nemus_render(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
    path: String,
    opts: RenderOpts,
) -> Result<String, AppError> {
    let cfg = nemus_config();
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
            return Err(AppError::Nemus(msg));
        }
    };

    // Offline render runs at a constant tempo. When a `tempo(...)` map is present
    // we render at its starting tempo (full offline tempo automation is a future
    // refinement); otherwise the script's `cps(...)`, else the configured default.
    let cps = output
        .tempo
        .points
        .first()
        .map(|p| p.1)
        .or(output.cps)
        .unwrap_or(cfg.default_cps);
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

/// List every downloadable sample pack (VSCO, Dirt-Samples, drum machines, …)
/// with its current install status.
#[tauri::command]
pub async fn nemus_packs() -> Result<Vec<packs::PackStatus>, AppError> {
    Ok(packs::list(&nemus_config()))
}

/// Start downloading + installing a sample pack by id (job-tracked). Returns the
/// job id; cancel via the standard `cancel_job`.
#[tauri::command]
pub async fn nemus_pack_download(
    app: AppHandle,
    pack_id: String,
) -> Result<String, AppError> {
    packs::start_download(&app, &nemus_config(), &pack_id).map_err(AppError::Nemus)
}

/// Delete an installed sample pack from disk (its whole install dir). No-op for
/// an unknown id; an already-absent pack succeeds. The caller re-reads the pack
/// list + sound registry afterwards.
#[tauri::command]
pub async fn nemus_pack_delete(pack_id: String) -> Result<(), AppError> {
    let cfg = nemus_config();
    // `remove_dir_all` on a multi-GB pack (VSCO) can run long past the UI's 50ms
    // budget — off the async worker via spawn_blocking.
    tauri::async_runtime::spawn_blocking(move || packs::delete(&cfg, &pack_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(AppError::Nemus)
}

/// Read the nemus config (`%APPDATA%\nemus\config.toml`).
#[tauri::command]
pub fn get_nemus_config() -> Result<NemusConfig, AppError> {
    Ok(nemus_config())
}

/// Persist a new nemus config. Takes effect for the next session / render.
#[tauri::command]
pub fn set_nemus_config(nemus: NemusConfig) -> Result<(), AppError> {
    config::save(&nemus).map_err(AppError::Other)
}

/// Tear down the nemus audio session for the app (window close). Safe to call
/// when nothing is running.
pub fn shutdown(app: &AppHandle) {
    if let Some(nemus) = app.try_state::<NemusState>() {
        nemus.shutdown();
    }
}
