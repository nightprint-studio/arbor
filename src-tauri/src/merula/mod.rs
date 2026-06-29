//! merula backend shell: the runtime that ties the merula crates
//! (`pattern`/`lang`/`audio`/`engine`, behind the `merula` facade) to the
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
//! - push throttled BE→FE events (`merula:diagnostics`/`active_haps`/`meters`/
//!   `transport`/`log`, [`events`]).

mod active_packs;
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
pub mod scenes;
mod sound_catalog;
pub mod sounds;
mod speech;
pub mod state;
mod validate;

use std::collections::HashSet;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tauri::{AppHandle, Manager, State};

use merula::prelude::{
    materialize_source, ControlMap, IslandKind, Scene, TempoMap, Time, TimeSpan, Tracks,
};

use crate::error::AppError;

pub use config::MerulaConfig;
use control::MerulaControl;
use events::{emit, MerulaDiagnostics, EVT_DIAGNOSTICS};
use render::RenderOpts;

// The additive Fase-4 commands live in their own modules (`query`/`sounds`/
// `state`/`project`, all `pub`). The invoke handler references them by full path
// (`merula::query::merula_query`, …) like the rest of the app's submodule commands
// (e.g. `commands::repo_commands::open_repo`) — a `#[tauri::command]` generates
// helper macros next to the fn, so a bare `pub use` of the fn would not surface
// them. `merula_set_track` stays defined inline below (referenced as
// `merula::merula_set_track`).

/// Per-window merula runtime state, managed in Tauri.
#[derive(Default)]
pub struct MerulaState {
    /// The live audio session (`None` until first play / after shutdown).
    session: Mutex<Option<Session>>,
    /// The most recent good evaluation, replayed when a session starts.
    latest: Mutex<Option<Latest>>,
}

/// A running audio session.
struct Session {
    tx: Sender<MerulaControl>,
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
    /// Launchable `scene(...)` declarations from the same evaluation, read by the
    /// clip launcher (`merula_scenes`) and substituted into `tracks` when fired.
    scenes: Vec<Scene>,
}

impl MerulaState {
    /// Return a sender to the audio thread, starting it if needed. Opening the
    /// session opens the audio device — done lazily here (on play), not on eval.
    fn ensure_session(&self, app: &AppHandle, cfg: &MerulaConfig) -> Sender<MerulaControl> {
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
            .name("merula-audio".to_string())
            .spawn(move || audio_thread::run(app2, rx, cfg2, loaded2))
            .expect("spawn merula-audio thread");
        *guard = Some(Session {
            tx: tx.clone(),
            handle,
            loaded,
        });
        tx
    }

    /// Send to the live session, if any. No-op when nothing is running.
    fn send_if_live(&self, msg: MerulaControl) {
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
        cfg: &MerulaConfig,
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
        // Speech sources count too: their content-addressed keys join the set so a
        // new `speech(...)` triggers a rebuild (which synthesizes + registers it).
        let speech_specs = validate::referenced_speech(&tracks);
        let mut referenced = validate::referenced_instruments(&tracks);
        referenced.extend(speech_specs.iter().map(|s| s.registry_key()));
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
                let specs2 = speech_specs.clone();
                match tauri::async_runtime::spawn_blocking(move || {
                    audio_thread::build_registry(&cfg2, &names2, &specs2)
                })
                .await
                {
                    Ok(registry) => Some(control::Prepared { registry, names }),
                    Err(e) => {
                        tracing::warn!("merula: registry decode task failed: {e}");
                        None
                    }
                }
            }
            None => None,
        };

        let _ = tx.send(MerulaControl::SetTracks { tracks, cps, tempo, prepared });
    }

    /// Play a preview arrangement (an instrument-preview snippet or an arbitrary
    /// user-selected chunk) on the audition bus for `cycles` cycles, decoding any
    /// referenced instrument **off the RT thread** first if the live registry
    /// doesn't resolve it yet (same path as [`Self::stage_tracks`]). No-op when no
    /// session is live (the caller opens one via [`Self::ensure_session`] first).
    async fn audition(&self, cfg: &MerulaConfig, tracks: Tracks<ControlMap>, cps: f64, cycles: u32) {
        let (tx, loaded) = {
            let guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref() {
                Some(s) if !s.handle.is_finished() => (s.tx.clone(), Arc::clone(&s.loaded)),
                _ => return,
            }
        };

        // Decode any referenced instrument off-thread when the live registry lacks it.
        // Speech sources join the set the same way as in `stage_tracks`.
        let speech_specs = validate::referenced_speech(&tracks);
        let mut referenced = validate::referenced_instruments(&tracks);
        referenced.extend(speech_specs.iter().map(|s| s.registry_key()));
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
                let specs2 = speech_specs.clone();
                match tauri::async_runtime::spawn_blocking(move || {
                    audio_thread::build_registry(&cfg2, &names2, &specs2)
                })
                .await
                {
                    Ok(registry) => Some(control::Prepared { registry, names }),
                    Err(e) => {
                        tracing::warn!("merula: audition decode task failed: {e}");
                        None
                    }
                }
            }
            None => None,
        };

        let _ = tx.send(MerulaControl::Audition { tracks, cps, cycles: cycles.max(1), prepared });
    }

    /// Tear the session down (drop the cpal stream on its thread) and join.
    /// Called on merula-window close.
    pub fn shutdown(&self) {
        let session = {
            let mut guard = self.session.lock().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(s) = session {
            let _ = s.tx.send(MerulaControl::Shutdown);
            let _ = s.handle.join();
        }
    }
}

/// Read merula's config from its own `%APPDATA%\merula\config.toml` (defaults on a
/// missing / corrupt file; never errors).
fn merula_config() -> MerulaConfig {
    config::load()
}

// ── Commands ──────────────────────────────────────────────────────────────────
//
// TODO(prune): merula moved to merula-be. These `#[tauri::command]` bodies are
// no longer registered in `handlers.rs` (the FE reaches merula through the `rpc`
// router → `merula-be`); they're kept dead-but-compiling for the non-destructive
// W6 cutover and can be deleted once merula-be has soaked. `MerulaState` and
// `config::migrate_legacy_dirs()` (launcher-boot) intentionally stay live.

/// Evaluate `.merula` source and stage it as the live arrangement. Returns
/// diagnostics (errors with span); language errors are diagnostics, not command
/// failures, so this still returns `Ok`. Does **not** open the audio device —
/// that happens on the first `play` (the staged result is replayed then).
#[tauri::command]
pub async fn merula_eval(
    app: AppHandle,
    merula: State<'_, MerulaState>,
    source: String,
    project_dir: Option<String>,
) -> Result<MerulaDiagnostics, AppError> {
    let cfg = merula_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(output) => {
            // Surface sound/instrument references the registry can't resolve as
            // editor errors (the renderer would silently fall back to the synth).
            // Done before the arrangement is moved to the live session below.
            let known = validate::known_instruments(&cfg);
            let mut errors = validate::validate_instruments(&output.tracks, &known);
            // Also flag speech controls (`.pitch`/`.rate`/…) chained onto a
            // non-speech source — a silent no-op, so a warning, not an error.
            errors.extend(validate::lint_speech_knobs(&source));
            let diagnostics = MerulaDiagnostics { errors };
            // Publish diagnostics to the editor *before* the (possibly slow) sample
            // staging below. Decoding newly-referenced sample voices can take a
            // while (a large `gm_` pack especially); if the emit waited for it, a
            // stale error from a mid-edit snapshot would linger on screen until the
            // decode finished. The editor's lint must never wait on audio.
            emit(&app, EVT_DIAGNOSTICS, diagnostics.clone());
            // Stash for replay on the next play, and push live if already running.
            {
                let mut latest = merula.latest.lock().unwrap_or_else(|e| e.into_inner());
                *latest = Some(Latest {
                    tracks: output.tracks.clone(),
                    cps: output.cps,
                    tempo: output.tempo.clone(),
                    scenes: output.scenes.clone(),
                });
            }
            // Push live if a session is running, decoding any new sample voices
            // off the RT thread (so editing while playing never freezes audio).
            merula.stage_tracks(&cfg, output.tracks, output.cps, output.tempo).await;
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
pub async fn merula_transport(
    app: AppHandle,
    merula: State<'_, MerulaState>,
    action: String,
    value: Option<f64>,
) -> Result<(), AppError> {
    match action.as_str() {
        "play" => {
            let cfg = merula_config();
            merula.ensure_session(&app, &cfg);
            // Feed the freshly-started transport the latest arrangement before
            // starting it (harmless if a live eval already pushed the same). Snapshot
            // it out from under the lock, then stage it — `stage_tracks` decodes any
            // sample voices off the RT thread, so the first play never stalls.
            let latest = {
                let guard = merula.latest.lock().unwrap_or_else(|e| e.into_inner());
                guard.as_ref().map(|l| (l.tracks.clone(), l.cps, l.tempo.clone()))
            };
            if let Some((tracks, cps, tempo)) = latest {
                merula.stage_tracks(&cfg, tracks, cps, tempo).await;
            }
            merula.send_if_live(MerulaControl::Play);
        }
        "stop" => merula.send_if_live(MerulaControl::Stop),
        "seek" => {
            merula.send_if_live(MerulaControl::Seek {
                cycle: value.unwrap_or(0.0),
            });
        }
        "set_cps" => {
            if let Some(cps) = value {
                merula.send_if_live(MerulaControl::SetCps { cps });
            }
        }
        other => return Err(AppError::Unsupported(format!("merula transport: {other}"))),
    }
    Ok(())
}

/// Push a **live mixer override** to the running session (no-op when stopped).
/// `action` ∈ `gain` | `pan` | `mute` | `solo` | `master_gain` | `reverb` |
/// `metronome` | `count_in`. These are ephemeral session tweaks: the next
/// `merula_eval` re-baselines the per-track mixer from the source (but `master_gain` /
/// `reverb` / `metronome` / `count_in` are global, source-less, so they persist across
/// evals). `value` is `0..1` for gain/pan/master_gain, `0|1` for mute/solo/metronome,
/// decay **seconds** for `reverb`, whole **bars** for `count_in`; `track` is ignored
/// for the global actions.
#[tauri::command]
pub async fn merula_set_track(
    merula: State<'_, MerulaState>,
    action: String,
    track: Option<u32>,
    value: f64,
) -> Result<(), AppError> {
    let msg = match action.as_str() {
        "gain" => MerulaControl::SetTrackGain {
            track: track.unwrap_or(0),
            gain: value as f32,
        },
        "pan" => MerulaControl::SetTrackPan {
            track: track.unwrap_or(0),
            pan: value as f32,
        },
        "mute" => MerulaControl::SetTrackMute {
            track: track.unwrap_or(0),
            mute: value != 0.0,
        },
        "solo" => MerulaControl::SetTrackSolo {
            track: track.unwrap_or(0),
            solo: value != 0.0,
        },
        "master_gain" => MerulaControl::SetMasterGain { gain: value as f32 },
        "reverb" => MerulaControl::SetReverb { seconds: value as f32 },
        "metronome" => MerulaControl::SetMetronome { on: value != 0.0 },
        "count_in" => MerulaControl::SetCountIn {
            bars: value.max(0.0) as u32,
        },
        other => return Err(AppError::Unsupported(format!("merula set_track: {other}"))),
    };
    merula.send_if_live(msg);
    Ok(())
}

/// Default preview tempo (cycles per second): one cycle of the snippet ≈ this many
/// seconds. A single `n(c4)` note then rings ~1s before its release.
const PREVIEW_CPS: f64 = 1.0;

/// Preview (audition) an instrument from a generated `.merula` **snippet**. The
/// front end composes a tiny expression — a note (or chord / scale degree) plus the
/// panel's knob/chain values, e.g. `n(c4).inst("synth.bass").gain(0.8).room(0.2)` —
/// and this evaluates it with the real language, then plays one cycle on a dedicated
/// preview bus that bypasses the song mixer (heard cleanly whether or not a song is
/// playing). Opens the audio device on demand. A malformed snippet simply doesn't
/// sound (no editor diagnostics). This single command never grows: every preview
/// capability rides on the language, not on new parameters.
#[tauri::command]
pub async fn merula_audition_expr(
    app: AppHandle,
    merula: State<'_, MerulaState>,
    expr: String,
    project_dir: Option<String>,
) -> Result<(), AppError> {
    let cfg = merula_config();
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

    merula.ensure_session(&app, &cfg);
    merula.audition(&cfg, output.tracks, cps, 1).await;
    Ok(())
}

/// Evaluate an arbitrary `.merula` chunk **in isolation** and return the events it
/// generates (plus its detected loop period + tempo), without touching the live
/// arrangement or the audio device. Powers the Scratch / expression evaluator: the
/// user pastes/selects a snippet and inspects what it produces. Errors come back
/// inline in [`query::SnippetEval`] (never on the `merula:diagnostics` channel — that
/// belongs to the main editor). The snippet is passed verbatim (no `tracks(...)`
/// wrapper), so the returned spans stay relative to the snippet text.
#[tauri::command]
pub async fn merula_eval_snippet(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
) -> Result<query::SnippetEval, AppError> {
    let cfg = merula_config();
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

/// Play an arbitrary `.merula` chunk **one-shot** on the audition bus: it sounds
/// once over its detected loop period and stops on its own, without disturbing the
/// song transport (the audition bus bypasses the song mixer and the voices
/// self-release). Powers right-click→Play on a selection, the Outline Play button,
/// and the Scratch panel. A malformed snippet simply doesn't sound. Opens the audio
/// device on demand. The snippet is passed verbatim (no wrapper), so it must be a
/// self-contained program (a `tracks(...)` / pattern expression).
#[tauri::command]
pub async fn merula_play_snippet(
    app: AppHandle,
    merula: State<'_, MerulaState>,
    source: String,
    project_dir: Option<String>,
) -> Result<(), AppError> {
    let cfg = merula_config();
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

    merula.ensure_session(&app, &cfg);
    merula.audition(&cfg, output.tracks, cps, cycles).await;
    Ok(())
}

/// **Freeze** a pattern: evaluate `source` (a self-contained snippet — the front
/// end prepends the file's constants/imports) and materialize the first track's
/// pattern over one cycle to canonical literal source (`n(c4 e4 g4)` / `s(bd sn)`),
/// the unit the editor splices back in to replace a generative expression with the
/// concrete notes it produces. Returns an empty string when the snippet doesn't
/// evaluate or yields no onsets (the caller leaves the source untouched). Pure
/// (no audio, no live state) — runs on the command thread.
#[tauri::command]
pub async fn merula_materialize(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
) -> Result<String, AppError> {
    let cfg = merula_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let output = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => return Ok(String::new()), // bad snippet → no-op (lints live elsewhere)
    };
    let Some(track) = output.tracks.tracks.first() else {
        return Ok(String::new());
    };
    // Freeze one cycle — the common case (euclid / random / chord generators are
    // per-cycle). A multi-cycle pattern captures its first cycle.
    let haps = track.pattern.query(TimeSpan::new(Time::int(0), Time::int(1)));
    if haps.is_empty() {
        return Ok(String::new());
    }
    // Note island when any onset carries a pitch; a sound island only when there
    // are sounds and no notes; default to notes (covers scale-degree patterns).
    let any_note = haps.iter().any(|h| h.value.note.is_some());
    let any_sound = haps.iter().any(|h| h.value.sound.is_some());
    let kind = if any_note || !any_sound { IslandKind::Note } else { IslandKind::Sound };
    Ok(materialize_source(kind, &haps))
}

/// Stop an in-flight snippet preview early (clears the audition bus only). The song
/// transport, if playing, is untouched. No-op when nothing is running.
#[tauri::command]
pub async fn merula_stop_snippet(merula: State<'_, MerulaState>) -> Result<(), AppError> {
    merula.send_if_live(MerulaControl::StopSnippet);
    Ok(())
}

/// Render `source` to a WAV file at `path` over `opts.cycles` cycles, on a
/// background job. Returns the job id. Evaluation errors fail the render (and are
/// emitted as diagnostics for the editor).
#[tauri::command]
pub async fn merula_render(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
    path: String,
    opts: RenderOpts,
) -> Result<String, AppError> {
    let cfg = merula_config();
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
            return Err(AppError::Merula(msg));
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
        opts.start_cycle.unwrap_or(0),
        opts.cycles,
        render_cfg,
        std::path::PathBuf::from(path),
    );
    Ok(job_id)
}

/// Render `source` to **per-track stems** (one WAV/OGG per track) in `dir`, on a
/// background job. Returns the job id. Like [`merula_render`] but bounces each
/// track in isolation; evaluation errors fail the export.
#[tauri::command]
pub async fn merula_render_stems(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
    dir: String,
    opts: RenderOpts,
) -> Result<String, AppError> {
    let cfg = merula_config();
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
            return Err(AppError::Merula(msg));
        }
    };

    let cps = output
        .tempo
        .points
        .first()
        .map(|p| p.1)
        .or(output.cps)
        .unwrap_or(cfg.default_cps);
    let render_cfg = render::resolve_config(cfg.render.render_config(), &opts);
    let job_id = render::spawn_render_stems(
        &app,
        output.tracks,
        cps,
        opts.start_cycle.unwrap_or(0),
        opts.cycles,
        render_cfg,
        std::path::PathBuf::from(dir),
    );
    Ok(job_id)
}

/// Result of a MIDI export: how many MIDI tracks + notes were written. Surfaced
/// to the front end for the export notification.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct MidiExportResult {
    /// MIDI tracks written (one per merula track that produced ≥1 note).
    pub tracks: u32,
    /// Total notes written across all tracks.
    pub notes: u32,
}

/// Export `source` to a Standard MIDI File at `path`, baking the arrangement's
/// natural loop period (one pass of the song). Unlike [`merula_render`] this is a
/// quick, note-only walk (no audio), so it resolves with the written summary
/// directly rather than running as a tracked job. Evaluation errors fail the
/// export (and are emitted as diagnostics for the editor).
#[tauri::command]
pub async fn merula_export_midi(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
    path: String,
) -> Result<MidiExportResult, AppError> {
    let cfg = merula_config();
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
            return Err(AppError::Merula(msg));
        }
    };

    // Same tempo choice as the WAV bounce (starting `tempo(...)` point, else the
    // script's `cps(...)`, else the configured default).
    let cps = output
        .tempo
        .points
        .first()
        .map(|p| p.1)
        .or(output.cps)
        .unwrap_or(cfg.default_cps);
    // Bake the arrangement's detected loop period — the whole song, once.
    let (_haps, _sections, loop_cycles) =
        query::collect_haps(&output.tracks, query::SNIPPET_WINDOW);
    let cycles = loop_cycles.max(1);

    // The walk + SMF write is quick, but keep it off the async worker (it can run
    // past the UI's 50ms budget on a long song); only the `Send` `Tracks` moves in.
    let tracks = output.tracks;
    let out = std::path::PathBuf::from(path);
    let summary = tauri::async_runtime::spawn_blocking(move || {
        merula::prelude::export_midi(&tracks, cps, cycles, &out)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
    .map_err(|e| AppError::Merula(e.to_string()))?;

    Ok(MidiExportResult { tracks: summary.tracks, notes: summary.notes })
}

/// One overload window from the offline level analysis (see [`merula_analyze_levels`]):
/// a track index plus the cycle range whose post-fader level exceeds 0 dBFS.
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

/// Analyze `source` for clipping **without playing it**: a silent offline render
/// over the arrangement's loop period that measures each track's post-fader peak
/// and reports where it exceeds 0 dBFS. Quick (no file IO, no audio output), run
/// off the async worker. A bad snippet resolves to an empty result (lints live
/// elsewhere), so it's safe to call live while editing.
#[tauri::command]
pub async fn merula_analyze_levels(
    app: AppHandle,
    source: String,
    project_dir: Option<String>,
) -> Result<LevelAnalysisResult, AppError> {
    let cfg = merula_config();
    let base = project_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let output = match eval::evaluate_source(&app, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => return Ok(LevelAnalysisResult { track_peaks: Vec::new(), clips: Vec::new() }),
    };

    // Same tempo + loop-period choices as the WAV / MIDI bounce, so the analysis
    // matches what an export would produce.
    let cps = output
        .tempo
        .points
        .first()
        .map(|p| p.1)
        .or(output.cps)
        .unwrap_or(cfg.default_cps);
    let (_haps, _sections, loop_cycles) =
        query::collect_haps(&output.tracks, query::SNIPPET_WINDOW);
    let cycles = loop_cycles.max(1);
    let sr = cfg.render.render_config().sample_rate;

    let tracks = output.tracks;
    let analysis = tauri::async_runtime::spawn_blocking(move || {
        merula::prelude::analyze_levels(&tracks, cps, cycles, sr)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

    Ok(LevelAnalysisResult {
        track_peaks: analysis.track_peaks,
        clips: analysis
            .clips
            .into_iter()
            .map(|c| ClipWindowDto { track: c.track, start: c.start_cycle, end: c.end_cycle, peak: c.peak })
            .collect(),
    })
}

/// List every downloadable sample pack (VSCO, Dirt-Samples, drum machines, …)
/// with its current install status.
#[tauri::command]
pub async fn merula_packs() -> Result<Vec<packs::PackStatus>, AppError> {
    Ok(packs::list(&merula_config()))
}

/// Start downloading + installing a sample pack by id (job-tracked). Returns the
/// job id; cancel via the standard `cancel_job`.
#[tauri::command]
pub async fn merula_pack_download(
    app: AppHandle,
    pack_id: String,
) -> Result<String, AppError> {
    packs::start_download(&app, &merula_config(), &pack_id).map_err(AppError::Merula)
}

/// Re-index an already-installed pack: rebuild its `registry.toml` from the
/// extracted files on disk (no re-download), refreshing the instruments it
/// exposes. Use after a pack indexed to zero instruments (e.g. an older VSCO
/// install). Returns the updated status; the caller re-reads packs + sounds.
#[tauri::command]
pub async fn merula_pack_reindex(pack_id: String) -> Result<packs::PackStatus, AppError> {
    let cfg = merula_config();
    // Walking the VSCO tree + writing every `_merula.sfz` runs far past the UI's
    // 50ms budget — off the async worker via spawn_blocking.
    tauri::async_runtime::spawn_blocking(move || packs::reindex(&cfg, &pack_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(AppError::Merula)
}

/// Delete an installed sample pack from disk (its whole install dir). No-op for
/// an unknown id; an already-absent pack succeeds. The caller re-reads the pack
/// list + sound registry afterwards.
#[tauri::command]
pub async fn merula_pack_delete(pack_id: String) -> Result<(), AppError> {
    let cfg = merula_config();
    // `remove_dir_all` on a multi-GB pack (VSCO) can run long past the UI's 50ms
    // budget — off the async worker via spawn_blocking.
    tauri::async_runtime::spawn_blocking(move || packs::delete(&cfg, &pack_id))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(AppError::Merula)
}

/// Toggle a pack's **active** state in the per-profile allow-list. Inactive packs
/// stay installed (pack management still sees them) but their instruments are
/// hidden from playback, the eval validator, and the sound bank. Seeds the
/// allow-list from the currently-installed packs on the first toggle, so turning
/// one pack off keeps every other installed pack on.
#[tauri::command]
pub async fn merula_pack_set_active(pack_id: String, active: bool) -> Result<(), AppError> {
    let cfg = merula_config();
    let installed_ids = packs::installed_ids(&cfg);
    active_packs::set_active(&pack_id, active, &installed_ids).map_err(AppError::Merula)
}

/// Read the merula config (`%APPDATA%\merula\config.toml`).
#[tauri::command]
pub fn get_merula_config() -> Result<MerulaConfig, AppError> {
    Ok(merula_config())
}

/// Persist a new merula config. Takes effect for the next session / render.
#[tauri::command]
pub fn set_merula_config(merula: MerulaConfig) -> Result<(), AppError> {
    config::save(&merula).map_err(AppError::Other)
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
pub fn merula_audio_devices() -> Result<Vec<AudioDeviceInfo>, AppError> {
    Ok(merula::prelude::list_output_devices()
        .into_iter()
        .map(|d| AudioDeviceInfo { name: d.name, is_default: d.is_default })
        .collect())
}

/// Choose the audio output device (by name; `None` = host default). Persists the
/// choice to the merula config and, when a session is live, switches it
/// immediately (reopening the stream, preserving the playhead + play state).
#[tauri::command]
pub fn merula_set_output_device(
    merula: State<'_, MerulaState>,
    device: Option<String>,
) -> Result<(), AppError> {
    let mut cfg = merula_config();
    cfg.output_device = device.clone();
    config::save(&cfg).map_err(AppError::Other)?;
    merula.send_if_live(MerulaControl::SetOutputDevice { device });
    Ok(())
}

/// Tear down the merula audio session for the app (window close). Safe to call
/// when nothing is running.
pub fn shutdown(app: &AppHandle) {
    if let Some(merula) = app.try_state::<MerulaState>() {
        merula.shutdown();
    }
}
