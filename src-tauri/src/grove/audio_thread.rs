//! The dedicated **grove audio thread**.
//!
//! One OS thread (never the job system, never the async runtime — hard rule:
//! the real-time path is sacred) owns the cpal [`OutputStream`] (which is `!Send`
//! and must live and die on the thread that opened it) and runs the look-ahead
//! driver: every ~tick it drains pending control messages, calls
//! [`Transport::tick`], and pushes throttled BE→FE events. The cpal callback runs
//! on its own internal thread; this thread only stays ~100 ms ahead of it.
//!
//! Lifecycle: spawned lazily on the first eval/play, torn down on `Shutdown`
//! (window close) — at which point dropping [`OutputStream`] here stops cpal.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use arbor_grove::prelude::{
    open_output_stream, AudioCommand, AudioSink, ControlMap, Epoch, Registry, StreamSink, Time,
    TimeSpan, Tracks, Transport,
};

use super::config::GroveConfig;
use super::control::GroveControl;
use super::events::{
    emit, ActiveHap, ActiveHaps, AudioErrorEvent, Meters, TransportState, EVT_ACTIVE_HAPS,
    EVT_AUDIO_ERROR, EVT_METERS, EVT_TRANSPORT,
};
use super::vsco;

/// Driver tick / control-drain cadence. Well under the 100 ms look-ahead (≈5×
/// headroom) so a missed wake never starves the scheduler.
const TICK_MS: u64 = 20;

/// Minimum spacing between `transport`/`meters` emissions (~30 fps). Decoupled
/// from the tick so a burst of control messages can't flood the front end.
const EMIT_INTERVAL: Duration = Duration::from_millis(33);

/// Run the audio session until a `Shutdown` (or the channel closing). Builds the
/// sound registry (a loaded VSCO manifest if installed, else the default synth
/// bank), opens the stream, then loops on the control channel. Returns when the
/// session ends; the caller's `JoinHandle` resolves then.
///
/// The registry is built **here**, on the owning thread, both because the
/// `Renderer` it backs then lives inside the cpal callback (no later swap) and to
/// keep it off the command thread.
pub fn run(app: AppHandle, rx: Receiver<GroveControl>, cfg: GroveConfig) {
    let init_cps = cfg.default_cps;
    let mut registry = vsco::load_registry(&cfg).unwrap_or_else(Registry::new);
    // The built-in `synth.*` presets are always available (no VSCO needed), so a
    // patch using `synth.lead`/`synth.bass`/… sounds as intended.
    registry.install_builtin_synths();

    // `_stream` keeps cpal alive; dropping it at function exit stops audio on
    // this (the owning) thread.
    let (sink, _stream) = match open_output_stream(Vec::new(), registry) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("grove: failed to open audio output: {e}");
            emit(
                &app,
                EVT_AUDIO_ERROR,
                AudioErrorEvent {
                    message: e.to_string(),
                },
            );
            return;
        }
    };

    let mut transport = Transport::new(sink, init_cps);
    // The shell keeps its own clone of the live arrangement so it can compute the
    // active-hap highlight (the transport does not expose its `Tracks`).
    let mut current: Tracks<ControlMap> = Tracks { tracks: Vec::new() };
    let mut last_haps: Vec<ActiveHap> = Vec::new();
    let mut last_emit = Instant::now();

    loop {
        // Block until a message or the tick interval elapses, then drain any
        // burst non-blocking so a multi-message update (SetTracks + Play) is
        // applied before the next tick.
        match rx.recv_timeout(Duration::from_millis(TICK_MS)) {
            Ok(msg) => {
                if !apply(&mut transport, &mut current, msg) {
                    break;
                }
                while let Ok(msg) = rx.try_recv() {
                    if !apply(&mut transport, &mut current, msg) {
                        return; // Shutdown drained mid-burst: drop _stream, exit.
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        transport.tick();

        // Throttled transport + meters.
        if last_emit.elapsed() >= EMIT_INTERVAL {
            emit_transport_and_meters(&app, &transport);
            last_emit = Instant::now();
        }

        // Active-hap highlight: on change only (cheap query each tick).
        let haps = active_haps(&transport, &current);
        if haps != last_haps {
            emit(&app, EVT_ACTIVE_HAPS, ActiveHaps { haps: haps.clone() });
            last_haps = haps;
        }
    }
}

/// Apply one control message. Returns `false` on `Shutdown` (caller exits).
fn apply(
    transport: &mut Transport<StreamSink>,
    current: &mut Tracks<ControlMap>,
    msg: GroveControl,
) -> bool {
    match msg {
        GroveControl::SetTracks { tracks, cps, tempo } => {
            *current = tracks.clone();
            transport.set_tracks(tracks);
            // A tempo-map drives the clock when present; otherwise the script's
            // constant `cps(...)` applies. Always push the map (empty clears any
            // previous automation back to a constant clock).
            let has_tempo = !tempo.is_empty();
            transport.set_tempo_map(tempo);
            if !has_tempo {
                if let Some(c) = cps {
                    transport.set_cps(c);
                }
            }
        }
        GroveControl::Play => transport.play(),
        GroveControl::Stop => transport.stop(),
        GroveControl::Seek { cycle } => {
            // Quantize the seek target to a whole cycle (the engine's `Time` is
            // rational; sub-cycle seeking is a future refinement).
            transport.seek(Time::int(cycle.round() as i64));
        }
        GroveControl::SetCps { cps } => transport.set_cps(cps),
        // Live mixer overrides: forwarded straight to the sink (non-blocking;
        // dropping the command on a full queue is acceptable for a knob drag).
        GroveControl::SetTrackGain { track, gain } => {
            let _ = transport.sink_mut().send(AudioCommand::SetTrackGain(track, gain));
        }
        GroveControl::SetTrackPan { track, pan } => {
            let _ = transport.sink_mut().send(AudioCommand::SetTrackPan(track, pan));
        }
        GroveControl::SetTrackMute { track, mute } => {
            let _ = transport.sink_mut().send(AudioCommand::SetTrackMute(track, mute));
        }
        GroveControl::SetTrackSolo { track, solo } => {
            let _ = transport.sink_mut().send(AudioCommand::SetTrackSolo(track, solo));
        }
        GroveControl::SetMasterGain { gain } => {
            let _ = transport.sink_mut().send(AudioCommand::SetMasterGain(gain));
        }
        GroveControl::Shutdown => return false,
    }
    true
}

/// Emit the current transport position and the audio telemetry (master +
/// per-track peak, voice count, DSP load).
fn emit_transport_and_meters(
    app: &AppHandle,
    transport: &Transport<StreamSink>,
) {
    let sink = transport.sink();
    let now = sink.now_frame();
    let sr = sink.sample_rate();
    let epoch = transport.epoch();

    emit(
        app,
        EVT_TRANSPORT,
        TransportState {
            playing: transport.is_playing(),
            // The transport freezes this when stopped (the sink clock keeps
            // running), so the FE ruler holds still after a stop.
            cycle: transport.position_cycle(),
            frame: now,
            cps: epoch.cps,
            sample_rate: sr,
        },
    );

    let m = sink.meters();
    emit(
        app,
        EVT_METERS,
        Meters {
            master: m.master,
            tracks: m.tracks,
            voices: m.voices,
            dsp_load: m.dsp_load,
        },
    );
}

/// Source spans of every hap sounding at the current playhead, for the editor
/// highlight. Queries the current integer cycle and keeps haps whose `whole`
/// (frame-mapped) brackets `now`. Empty while stopped.
fn active_haps(
    transport: &Transport<StreamSink>,
    current: &Tracks<ControlMap>,
) -> Vec<ActiveHap> {
    if !transport.is_playing() {
        return Vec::new();
    }
    let sink = transport.sink();
    let now = sink.now_frame();
    let sr = sink.sample_rate();
    let epoch: Epoch = transport.epoch();

    let cyc = epoch.cycle_of(now, sr).floor() as i64;
    let span = TimeSpan::new(Time::int(cyc), Time::int(cyc + 1));

    let mut haps: Vec<ActiveHap> = Vec::new();
    for (track_idx, track) in current.tracks.iter().enumerate() {
        let track_id = track_idx as u32;
        for hap in track.pattern.query(span) {
            let Some(src) = hap.span else { continue };
            // Use `whole` (the event's full extent) when present; a continuous
            // signal (no onset) has none and is skipped for highlight.
            let Some(whole) = hap.whole else { continue };
            let begin = epoch.frame_of(whole.begin, sr);
            let end = epoch.frame_of(whole.end, sr);
            if begin <= now && now < end {
                let entry = ActiveHap {
                    start: src.start,
                    end: src.end,
                    track: track_id,
                };
                if !haps.contains(&entry) {
                    haps.push(entry);
                }
            }
        }
    }
    // Stable order so the on-change comparison only fires on a real set change.
    haps.sort_unstable_by_key(|h| (h.track, h.start, h.end));
    haps
}
