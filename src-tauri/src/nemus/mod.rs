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
pub mod format;
pub mod import;
pub mod libraries;
pub mod models;
mod packs;
pub mod project;
pub mod query;
pub mod reference;
mod render;
pub mod scales;
mod sound_catalog;
pub mod sounds;
pub mod state;
mod validate;

use std::collections::HashSet;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
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
    /// Instruments decoded into the live stream's registry (built-in synths +
    /// sample voices). Shared with the audio thread: the command reads it to tell
    /// whether an eval pulls in a *new* voice (which it then decodes off-thread,
    /// handing the result over in `SetTracks`); the audio thread updates it after
    /// a successful stream swap.
    loaded: Arc<Mutex<HashSet<String>>>,
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
        // Seed the shared `loaded` set with the always-present built-in synth names
        // (so a synth-only patch never triggers a pointless decode); pack voices
        // join it as they're first referenced.
        let loaded = Arc::new(Mutex::new(audio_thread::builtin_synth_names()));
        let loaded2 = Arc::clone(&loaded);
        let handle = std::thread::Builder::new()
            .name("nemus-audio".to_string())
            .spawn(move || audio_thread::run(app2, rx, cfg2, loaded2))
            .expect("spawn nemus-audio thread");
        *guard = Some(Session {
            tx: tx.clone(),
            handle,
            loaded,
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

    /// Stage an arrangement on the live session, decoding any newly-referenced
    /// sample instruments **off the RT thread** first. Reads the session's shared
    /// `loaded` set: if the arrangement only uses already-loaded voices it just
    /// restages the tracks (no rebuild); otherwise it builds the wider registry on
    /// a blocking worker and hands it to the audio thread ready to swap in — so the
    /// seconds-long decode never freezes playback. No-op when no session is live.
    async fn stage_tracks(
        &self,
        cfg: &NemusConfig,
        tracks: Tracks<ControlMap>,
        cps: Option<f64>,
        tempo: TempoMap,
    ) {
        // Snapshot the sender + shared `loaded` under the lock, then release it
        // before any `.await` (the `MutexGuard` is not `Send`).
        let (tx, loaded) = {
            let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref() {
                Some(s) if !s.handle.is_finished() => (s.tx.clone(), Arc::clone(&s.loaded)),
                _ => return,
            }
        };

        // Does this arrangement pull in a voice the live registry doesn't have yet?
        let referenced = validate::referenced_instruments(&tracks);
        let target: Option<HashSet<String>> = {
            let have = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if referenced.is_subset(&have) {
                None
            } else {
                Some(have.union(&referenced).cloned().collect())
            }
        };

        // New voice → decode the wider set on a blocking worker (never the RT
        // thread), hand the ready registry to the audio thread. Old voices only →
        // restage with no rebuild.
        let prepared = match target {
            Some(names) => {
                let cfg2 = cfg.clone();
                let names2 = names.clone();
                match tauri::async_runtime::spawn_blocking(move || {
                    audio_thread::build_registry(&cfg2, &names2)
                })
                .await
                {
                    Ok(registry) => Some(control::Prepared { registry, names }),
                    Err(e) => {
                        tracing::warn!("nemus: registry decode task failed: {e}");
                        None
                    }
                }
            }
            None => None,
        };

        let _ = tx.send(NemusControl::SetTracks { tracks, cps, tempo, prepared });
    }

    /// Play a preview arrangement (an instrument-preview snippet or an arbitrary
    /// user-selected chunk) on the audition bus for `cycles` cycles, decoding any
    /// referenced instrument **off the RT thread** first if the live registry
    /// doesn't resolve it yet (same path as [`Self::stage_tracks`]). No-op when no
    /// session is live (the caller opens one via [`Self::ensure_session`] first).
    async fn audition(&self, cfg: &NemusConfig, tracks: Tracks<ControlMap>, cps: f64, cycles: u32) {
        let (tx, loaded) = {
            let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref() {
                Some(s) if !s.handle.is_finished() => (s.tx.clone(), Arc::clone(&s.loaded)),
                _ => return,
            }
        };

        // Decode any referenced instrument off-thread when the live registry lacks it.
        let referenced = validate::referenced_instruments(&tracks);
        let target: Option<HashSet<String>> = {
            let have = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if referenced.is_subset(&have) {
                None
            } else {
                Some(have.union(&referenced).cloned().collect())
            }
        };
        let prepared = match target {
            Some(names) => {
                let cfg2 = cfg.clone();
                let names2 = names.clone();
                match tauri::async_runtime::spawn_blocking(move || {
                    audio_thread::build_registry(&cfg2, &names2)
                })
                .await
                {
                    Ok(registry) => Some(control::Prepared { registry, names }),
                    Err(e) => {
                        tracing::warn!("nemus: audition decode task failed: {e}");
                        None
                    }
                }
            }
            None => None,
        };

        let _ = tx.send(NemusControl::Audition { tracks, cps, cycles: cycles.max(1), prepared });
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

    match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(output) => {
            // Surface sound/instrument references the registry can't resolve as
            // editor errors (the renderer would silently fall back to the synth).
            // Done before the arrangement is moved to the live session below.
            let known = validate::known_instruments(&cfg);
            let errors = validate::validate_instruments(&output.tracks, &known);
            let diagnostics = NemusDiagnostics { errors };
            // Publish diagnostics to the editor *before* the (possibly slow) sample
            // staging below. Decoding newly-referenced sample voices can take a
            // while (a large `gm_` pack especially); if the emit waited for it, a
            // stale error from a mid-edit snapshot would linger on screen until the
            // decode finished. The editor's lint must never wait on audio.
            emit(&app, EVT_DIAGNOSTICS, diagnostics.clone());
            // Stash for replay on the next play, and push live if already running.
            {
                let mut latest = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
                *latest = Some(Latest {
                    tracks: output.tracks.clone(),
                    cps: output.cps,
                    tempo: output.tempo.clone(),
                });
            }
            // Push live if a session is running, decoding any new sample voices
            // off the RT thread (so editing while playing never freezes audio).
            nemus.stage_tracks(&cfg, output.tracks, output.cps, output.tempo).await;
            Ok(diagnostics)
        }
        Err(diags) => {
            emit(&app, EVT_DIAGNOSTICS, diags.clone());
            Ok(diags)
        }
    }
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
            nemus.ensure_session(&app, &cfg);
            // Feed the freshly-started transport the latest arrangement before
            // starting it (harmless if a live eval already pushed the same). Snapshot
            // it out from under the lock, then stage it — `stage_tracks` decodes any
            // sample voices off the RT thread, so the first play never stalls.
            let latest = {
                let guard = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
                guard.as_ref().map(|l| (l.tracks.clone(), l.cps, l.tempo.clone()))
            };
            if let Some((tracks, cps, tempo)) = latest {
                nemus.stage_tracks(&cfg, tracks, cps, tempo).await;
            }
            nemus.send_if_live(NemusControl::Play);
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

/// Default preview tempo (cycles per second): one cycle of the snippet ≈ this many
/// seconds. A single `n(c4)` note then rings ~1s before its release.
const PREVIEW_CPS: f64 = 1.0;

/// Preview (audition) an instrument from a generated `.nemus` **snippet**. The
/// front end composes a tiny expression — a note (or chord / scale degree) plus the
/// panel's knob/chain values, e.g. `n(c4).inst("synth.bass").gain(0.8).room(0.2)` —
/// and this evaluates it with the real language, then plays one cycle on a dedicated
/// preview bus that bypasses the song mixer (heard cleanly whether or not a song is
/// playing). Opens the audio device on demand. A malformed snippet simply doesn't
/// sound (no editor diagnostics). This single command never grows: every preview
/// capability rides on the language, not on new parameters.
#[tauri::command]
pub async fn nemus_audition_expr(
    app: AppHandle,
    nemus: State<'_, NemusState>,
    expr: String,
    project_dir: Option<String>,
) -> Result<(), AppError> {
    let cfg = nemus_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    // Wrap the snippet as a one-track program and evaluate it with the real lang.
    let source = format!("tracks(track(\"preview\", {expr}))");
    let output = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => return Ok(()), // bad snippet → silent (no diagnostics surfaced)
    };
    let cps = output.cps.unwrap_or(PREVIEW_CPS);

    nemus.ensure_session(&app, &cfg);
    nemus.audition(&cfg, output.tracks, cps, 1).await;
    Ok(())
}

/// Evaluate an arbitrary `.nemus` chunk **in isolation** and return the events it
/// generates (plus its detected loop period + tempo), without touching the live
/// arrangement or the audio device. Powers the Scratch / expression evaluator: the
/// user pastes/selects a snippet and inspects what it produces. Errors come back
/// inline in [`query::SnippetEval`] (never on the `nemus:diagnostics` channel — that
/// belongs to the main editor). The snippet is passed verbatim (no `tracks(...)`
/// wrapper), so the returned spans stay relative to the snippet text.
#[tauri::command]
pub async fn nemus_eval_snippet(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
) -> Result<query::SnippetEval, AppError> {
    let cfg = nemus_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(output) => {
            let known = validate::known_instruments(&cfg);
            let diagnostics = validate::validate_instruments(&output.tracks, &known);
            let (haps, sections, loop_cycles) =
                query::collect_haps(&output.tracks, query::SNIPPET_WINDOW);
            let cps = output.tempo.points.first().map(|p| p.1).or(output.cps);
            Ok(query::SnippetEval { diagnostics, haps, sections, loop_cycles, cps })
        }
        Err(diags) => Ok(query::SnippetEval {
            diagnostics: diags.errors,
            haps: Vec::new(),
            sections: Vec::new(),
            loop_cycles: 0,
            cps: None,
        }),
    }
}

/// Play an arbitrary `.nemus` chunk **one-shot** on the audition bus: it sounds
/// once over its detected loop period and stops on its own, without disturbing the
/// song transport (the audition bus bypasses the song mixer and the voices
/// self-release). Powers right-click→Play on a selection, the Outline Play button,
/// and the Scratch panel. A malformed snippet simply doesn't sound. Opens the audio
/// device on demand. The snippet is passed verbatim (no wrapper), so it must be a
/// self-contained program (a `tracks(...)` / pattern expression).
#[tauri::command]
pub async fn nemus_play_snippet(
    app: AppHandle,
    nemus: State<'_, NemusState>,
    source: String,
    project_dir: Option<String>,
) -> Result<(), AppError> {
    let cfg = nemus_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let output = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => return Ok(()), // bad snippet → silent (Scratch panel surfaces errors)
    };
    // One-shot length = the snippet's detected loop period (clamp ≥ 1 cycle).
    let (_haps, _sections, loop_cycles) =
        query::collect_haps(&output.tracks, query::SNIPPET_WINDOW);
    let cycles = loop_cycles.max(1);
    let cps = output.tempo.points.first().map(|p| p.1).or(output.cps).unwrap_or(PREVIEW_CPS);

    nemus.ensure_session(&app, &cfg);
    nemus.audition(&cfg, output.tracks, cps, cycles).await;
    Ok(())
}

/// Stop an in-flight snippet preview early (clears the audition bus only). The song
/// transport, if playing, is untouched. No-op when nothing is running.
#[tauri::command]
pub async fn nemus_stop_snippet(nemus: State<'_, NemusState>) -> Result<(), AppError> {
    nemus.send_if_live(NemusControl::StopSnippet);
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

/// Re-index an already-installed pack: rebuild its `registry.toml` from the
/// extracted files on disk (no re-download), refreshing the instruments it
/// exposes. Use after a pack indexed to zero instruments (e.g. an older VSCO
/// install). Returns the updated status; the caller re-reads packs + sounds.
#[tauri::command]
pub async fn nemus_pack_reindex(pack_id: String) -> Result<packs::PackStatus, AppError> {
    let cfg = nemus_config();
    // Walking the VSCO tree + writing every `_nemus.sfz` runs far past the UI's
    // 50ms budget — off the async worker via spawn_blocking.
    tauri::async_runtime::spawn_blocking(move || packs::reindex(&cfg, &pack_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(AppError::Nemus)
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

/// One selectable audio output device, for the Settings picker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDeviceInfo {
    /// cpal device name — the stable id persisted + handed back when chosen.
    pub name: String,
    /// Whether this is the host's current default output device.
    pub is_default: bool,
}

/// List the host's audio output devices (name + whether it's the system default).
/// The default is always reachable by selecting "System default" (a `None` device).
#[tauri::command]
pub fn nemus_audio_devices() -> Result<Vec<AudioDeviceInfo>, AppError> {
    Ok(arbor_nemus::prelude::list_output_devices()
        .into_iter()
        .map(|d| AudioDeviceInfo { name: d.name, is_default: d.is_default })
        .collect())
}

/// Choose the audio output device (by name; `None` = host default). Persists the
/// choice to the nemus config and, when a session is live, switches it
/// immediately (reopening the stream, preserving the playhead + play state).
#[tauri::command]
pub fn nemus_set_output_device(
    nemus: State<'_, NemusState>,
    device: Option<String>,
) -> Result<(), AppError> {
    let mut cfg = nemus_config();
    cfg.output_device = device.clone();
    config::save(&cfg).map_err(AppError::Other)?;
    nemus.send_if_live(NemusControl::SetOutputDevice { device });
    Ok(())
}

/// Tear down the nemus audio session for the app (window close). Safe to call
/// when nothing is running.
pub fn shutdown(app: &AppHandle) {
    if let Some(nemus) = app.try_state::<NemusState>() {
        nemus.shutdown();
    }
}
