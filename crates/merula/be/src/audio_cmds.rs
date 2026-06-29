//! Transport + live-mixer + audition command handlers.
//!
//! The RPC surface that drives the audio session: evaluate-and-stage, transport
//! control, live mixer overrides, and the audition (preview / snippet) bus.
//! Ported from the shell's `src-tauri/src/merula/mod.rs` commands, with the egress
//! changed from `AppHandle` + `State<MerulaState>` to the single `&MerulaState`
//! context (W0). The off-thread sample decode that the shell ran via
//! `tauri::async_runtime::spawn_blocking` runs here on `tokio::task::spawn_blocking`
//! (the backend runtime, as corvus-be does) — NEVER `tauri::async_runtime` (this
//! crate is Tauri-free).
//!
//! The audio device opens lazily on the first `play`/audition, never on eval; the
//! session orchestration (lazy start / send / off-thread staging) lives in
//! [`crate::session`].

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use merula::prelude::{ControlMap, TempoMap, Tracks};

use crate::config_cmds::{self, MerulaConfig};
use crate::control::{self, MerulaControl};
use crate::eval::validate;
use crate::events::{emit, MerulaDiagnostics, EVT_DIAGNOSTICS};
use crate::session::{self, Latest};
use crate::state::MerulaState;

/// Default preview tempo (cycles per second): one cycle of the snippet ≈ this many
/// seconds. A single `n(c4)` note then rings ~1s before its release.
const PREVIEW_CPS: f64 = 1.0;

/// Resolve the project base dir from the optional `project_dir` argument, defaulting
/// to the current directory (the shell's identical fallback).
fn base_dir(project_dir: Option<String>) -> PathBuf {
    project_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Stage an arrangement on the live session, decoding any newly-referenced sample
/// instruments **off the RT thread** first. Reads the session's shared `loaded`
/// set: if the arrangement only uses already-loaded voices it just restages the
/// tracks (no rebuild); otherwise it builds the wider registry on a blocking worker
/// and hands it to the audio thread ready to swap in — so the seconds-long decode
/// never freezes playback. No-op when no session is live.
async fn stage_tracks(
    ctx: &MerulaState,
    cfg: &MerulaConfig,
    tracks: Tracks<ControlMap>,
    cps: Option<f64>,
    tempo: TempoMap,
) {
    // Snapshot the sender + shared `loaded` under the lock, then release it before
    // any `.await` (the `MutexGuard` is not `Send`).
    let Some((tx, loaded)) = ({
        let guard = ctx.session();
        session::live_handles(&guard)
    }) else {
        return;
    };

    let prepared = build_prepared(cfg, &tracks, &loaded).await;
    let _ = tx.send(MerulaControl::SetTracks { tracks, cps, tempo, prepared });
}

/// Play a preview arrangement on the audition bus for `cycles` cycles, decoding any
/// referenced instrument **off the RT thread** first if the live registry doesn't
/// resolve it yet (same path as [`stage_tracks`]). No-op when no session is live
/// (the caller opens one via [`crate::session::ensure`] first).
async fn audition(
    ctx: &MerulaState,
    cfg: &MerulaConfig,
    tracks: Tracks<ControlMap>,
    cps: f64,
    cycles: u32,
) {
    let Some((tx, loaded)) = ({
        let guard = ctx.session();
        session::live_handles(&guard)
    }) else {
        return;
    };

    let prepared = build_prepared(cfg, &tracks, &loaded).await;
    let _ = tx.send(MerulaControl::Audition { tracks, cps, cycles: cycles.max(1), prepared });
}

/// The shared off-thread decode step behind both [`stage_tracks`] and [`audition`]:
/// does this arrangement pull in a voice (or `speech(...)` source) the live registry
/// doesn't have yet? If so, decode the wider set on a blocking worker (never the RT
/// thread) and return the ready [`control::Prepared`]; otherwise `None` (restage with
/// no rebuild). Speech sources count too: their content-addressed keys join the set
/// so a new `speech(...)` triggers a rebuild (which synthesizes + registers it).
async fn build_prepared(
    cfg: &MerulaConfig,
    tracks: &Tracks<ControlMap>,
    loaded: &Arc<Mutex<HashSet<String>>>,
) -> Option<control::Prepared> {
    let speech_specs = validate::referenced_speech(tracks);
    let mut referenced = validate::referenced_instruments(tracks);
    referenced.extend(speech_specs.iter().map(|s| s.registry_key()));
    let target: Option<HashSet<String>> = {
        let have = loaded.lock().unwrap_or_else(|e| e.into_inner());
        if referenced.is_subset(&have) {
            None
        } else {
            Some(have.union(&referenced).cloned().collect())
        }
    };

    let names = target?;
    let cfg2 = cfg.clone();
    let names2 = names.clone();
    let specs2 = speech_specs;
    match tokio::task::spawn_blocking(move || {
        crate::audio_thread::build_registry(&cfg2, &names2, &specs2)
    })
    .await
    {
        Ok(registry) => Some(control::Prepared { registry, names }),
        Err(e) => {
            eprintln!("merula: registry decode task failed: {e}");
            None
        }
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Evaluate `.merula` source and stage it as the live arrangement. Returns
/// diagnostics (errors with span); language errors are diagnostics, not command
/// failures, so this still returns `Ok`. Does **not** open the audio device —
/// that happens on the first `play` (the staged result is replayed then).
#[arbor_rpc::handler]
async fn merula_eval(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
) -> Result<MerulaDiagnostics, String> {
    let cfg = config_cmds::load();
    let base = base_dir(project_dir);
    let sink = ctx.event_sink();

    match crate::eval::evaluate_source(Arc::clone(&sink), &source, base, cfg.eval_config()) {
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
            emit(&*sink, EVT_DIAGNOSTICS, diagnostics.clone());
            // Stash for replay on the next play, and push live if already running.
            session::set_latest(Latest {
                tracks: output.tracks.clone(),
                cps: output.cps,
                tempo: output.tempo.clone(),
                scenes: output.scenes.clone(),
            });
            // Push live if a session is running, decoding any new sample voices
            // off the RT thread (so editing while playing never freezes audio).
            stage_tracks(ctx, &cfg, output.tracks, output.cps, output.tempo).await;
            Ok(diagnostics)
        }
        Err(diags) => {
            emit(&*sink, EVT_DIAGNOSTICS, diags.clone());
            Ok(diags)
        }
    }
}

/// Transport control. `action` ∈ `play` | `stop` | `seek` | `set_cps`; `value`
/// carries the target cycle (`seek`) or tempo (`set_cps`).
#[arbor_rpc::handler]
async fn merula_transport(
    ctx: &MerulaState,
    action: String,
    value: Option<f64>,
) -> Result<(), String> {
    match action.as_str() {
        "play" => {
            let cfg = config_cmds::load();
            // Lazy-start the session (opens the audio device) under the slot guard.
            {
                let mut guard = ctx.session();
                session::ensure(&mut guard, ctx.event_sink(), &cfg);
            }
            // Feed the freshly-started transport the latest arrangement before
            // starting it (harmless if a live eval already pushed the same). Snapshot
            // it from the typed latest store, then stage it — `stage_tracks` decodes
            // any sample voices off the RT thread, so the first play never stalls.
            let latest = session::with_latest(|l| (l.tracks.clone(), l.cps, l.tempo.clone()));
            if let Some((tracks, cps, tempo)) = latest {
                stage_tracks(ctx, &cfg, tracks, cps, tempo).await;
            }
            send_if_live(ctx, MerulaControl::Play);
        }
        "stop" => send_if_live(ctx, MerulaControl::Stop),
        "seek" => send_if_live(ctx, MerulaControl::Seek { cycle: value.unwrap_or(0.0) }),
        "set_cps" => {
            if let Some(cps) = value {
                send_if_live(ctx, MerulaControl::SetCps { cps });
            }
        }
        other => return Err(format!("merula transport: {other}")),
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
#[arbor_rpc::handler]
fn merula_set_track(
    ctx: &MerulaState,
    action: String,
    track: Option<u32>,
    value: f64,
) -> Result<(), String> {
    let msg = match action.as_str() {
        "gain" => MerulaControl::SetTrackGain { track: track.unwrap_or(0), gain: value as f32 },
        "pan" => MerulaControl::SetTrackPan { track: track.unwrap_or(0), pan: value as f32 },
        "mute" => MerulaControl::SetTrackMute { track: track.unwrap_or(0), mute: value != 0.0 },
        "solo" => MerulaControl::SetTrackSolo { track: track.unwrap_or(0), solo: value != 0.0 },
        "master_gain" => MerulaControl::SetMasterGain { gain: value as f32 },
        "reverb" => MerulaControl::SetReverb { seconds: value as f32 },
        "metronome" => MerulaControl::SetMetronome { on: value != 0.0 },
        "count_in" => MerulaControl::SetCountIn { bars: value.max(0.0) as u32 },
        other => return Err(format!("merula set_track: {other}")),
    };
    send_if_live(ctx, msg);
    Ok(())
}

/// Preview (audition) an instrument from a generated `.merula` **snippet**. The
/// front end composes a tiny expression — a note (or chord / scale degree) plus the
/// panel's knob/chain values, e.g. `n(c4).inst("synth.bass").gain(0.8).room(0.2)` —
/// and this evaluates it with the real language, then plays one cycle on a dedicated
/// preview bus that bypasses the song mixer (heard cleanly whether or not a song is
/// playing). Opens the audio device on demand. A malformed snippet simply doesn't
/// sound (no editor diagnostics). This single command never grows: every preview
/// capability rides on the language, not on new parameters.
#[arbor_rpc::handler]
async fn merula_audition_expr(
    ctx: &MerulaState,
    expr: String,
    project_dir: Option<String>,
) -> Result<(), String> {
    let cfg = config_cmds::load();
    let base = base_dir(project_dir);
    let sink = ctx.event_sink();

    // Wrap the snippet as a one-track program and evaluate it with the real lang.
    let source = format!("tracks(track(\"preview\", {expr}))");
    let output = match crate::eval::evaluate_source(sink, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => return Ok(()), // bad snippet → silent (no diagnostics surfaced)
    };
    let cps = output.cps.unwrap_or(PREVIEW_CPS);

    {
        let mut guard = ctx.session();
        session::ensure(&mut guard, ctx.event_sink(), &cfg);
    }
    audition(ctx, &cfg, output.tracks, cps, 1).await;
    Ok(())
}

/// Evaluate an arbitrary `.merula` chunk **in isolation** and return the events it
/// generates (plus its detected loop period + tempo), without touching the live
/// arrangement or the audio device. Powers the Scratch / expression evaluator: the
/// user pastes/selects a snippet and inspects what it produces. Errors come back
/// inline in [`crate::query::SnippetEval`] (never on the `merula:diagnostics`
/// channel — that belongs to the main editor). The snippet is passed verbatim (no
/// `tracks(...)` wrapper), so the returned spans stay relative to the snippet text.
#[arbor_rpc::handler]
async fn merula_eval_snippet(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
) -> Result<crate::query::SnippetEval, String> {
    let cfg = config_cmds::load();
    let base = base_dir(project_dir);
    let sink = ctx.event_sink();

    match crate::eval::evaluate_source(sink, &source, base, cfg.eval_config()) {
        Ok(output) => {
            let known = validate::known_instruments(&cfg);
            let diagnostics = validate::validate_instruments(&output.tracks, &known);
            let (haps, sections, loop_cycles) =
                crate::query::collect_haps(&output.tracks, crate::query::SNIPPET_WINDOW);
            let cps = output.tempo.points.first().map(|p| p.1).or(output.cps);
            Ok(crate::query::SnippetEval { diagnostics, haps, sections, loop_cycles, cps })
        }
        Err(diags) => Ok(crate::query::SnippetEval {
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
#[arbor_rpc::handler]
async fn merula_play_snippet(
    ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
) -> Result<(), String> {
    let cfg = config_cmds::load();
    let base = base_dir(project_dir);
    let sink = ctx.event_sink();

    let output = match crate::eval::evaluate_source(sink, &source, base, cfg.eval_config()) {
        Ok(o) => o,
        Err(_diags) => return Ok(()), // bad snippet → silent (Scratch panel surfaces errors)
    };
    // One-shot length = the snippet's detected loop period (clamp ≥ 1 cycle).
    let (_haps, _sections, loop_cycles) =
        crate::query::collect_haps(&output.tracks, crate::query::SNIPPET_WINDOW);
    let cycles = loop_cycles.max(1);
    let cps = output.tempo.points.first().map(|p| p.1).or(output.cps).unwrap_or(PREVIEW_CPS);

    {
        let mut guard = ctx.session();
        session::ensure(&mut guard, ctx.event_sink(), &cfg);
    }
    audition(ctx, &cfg, output.tracks, cps, cycles).await;
    Ok(())
}

/// Stop an in-flight snippet preview early (clears the audition bus only). The song
/// transport, if playing, is untouched. No-op when nothing is running.
#[arbor_rpc::handler]
fn merula_stop_snippet(ctx: &MerulaState) -> Result<(), String> {
    send_if_live(ctx, MerulaControl::StopSnippet);
    Ok(())
}

/// Send a control message to the live session, scoping the (non-`Send`) session
/// guard so it is dropped before returning. No-op when no session is running.
fn send_if_live(ctx: &MerulaState, msg: MerulaControl) {
    let guard = ctx.session();
    session::send_if_live(&guard, msg);
}
