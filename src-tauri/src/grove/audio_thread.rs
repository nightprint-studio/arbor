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

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use arbor_grove::prelude::{
    open_output_stream, AudioCommand, AudioError, AudioSink, ControlMap, Epoch, OutputStream,
    Registry, StreamSink, TempoMap, Time, TimeSpan, Tracks, Transport,
};

use super::config::GroveConfig;
use super::control::GroveControl;
use super::events::{
    emit, ActiveHap, ActiveHaps, AudioErrorEvent, Meters, TransportState, EVT_ACTIVE_HAPS,
    EVT_AUDIO_ERROR, EVT_METERS, EVT_TRANSPORT,
};
use super::packs;

/// Driver tick / control-drain cadence. Well under the 100 ms look-ahead (≈5×
/// headroom) so a missed wake never starves the scheduler.
const TICK_MS: u64 = 20;

/// Minimum spacing between `transport`/`meters` emissions (~30 fps). Decoupled
/// from the tick so a burst of control messages can't flood the front end.
const EMIT_INTERVAL: Duration = Duration::from_millis(33);

/// Run the audio session until a `Shutdown` (or the channel closing). Opens the
/// stream with the built-in synths only, then **lazily** decodes each sample
/// instrument the first time an arrangement references it (rebuilding the stream
/// to merge the new voices in). Loops on the control channel; returns when the
/// session ends and the caller's `JoinHandle` resolves.
///
/// Why lazy: a pack like VSCO or Dirt-Samples is gigabytes of audio. Decoding
/// every installed pack up front made the first play stall and held all of it in
/// RAM even for a one-drum patch — now only the referenced instruments load.
pub fn run(app: AppHandle, rx: Receiver<GroveControl>, cfg: GroveConfig) {
    // Start with synths only (always available, no decode); sample voices load on
    // first reference.
    let mut session = match Session::open(&cfg, &HashSet::new()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("grove: failed to open audio output: {e}");
            emit(&app, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
            return;
        }
    };

    let mut last_haps: Vec<ActiveHap> = Vec::new();
    let mut last_emit = Instant::now();

    loop {
        // Block until a message or the tick interval elapses, then drain any
        // burst non-blocking so a multi-message update (SetTracks + Play) is
        // applied before the next tick.
        match rx.recv_timeout(Duration::from_millis(TICK_MS)) {
            Ok(msg) => {
                if !session.apply(&app, &cfg, msg) {
                    break;
                }
                while let Ok(msg) = rx.try_recv() {
                    if !session.apply(&app, &cfg, msg) {
                        return; // Shutdown drained mid-burst: drop stream, exit.
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        session.transport.tick();

        // Throttled transport + meters.
        if last_emit.elapsed() >= EMIT_INTERVAL {
            emit_transport_and_meters(&app, &session.transport);
            last_emit = Instant::now();
        }

        // Active-hap highlight: on change only (cheap query each tick).
        let haps = active_haps(&session.transport, &session.current);
        if haps != last_haps {
            emit(&app, EVT_ACTIVE_HAPS, ActiveHaps { haps: haps.clone() });
            last_haps = haps;
        }
    }
}

/// Build a cpal stream whose registry is the built-in synths plus exactly the
/// `needed` sample instruments (decoded from their installed packs — synth names
/// in the set are ignored by the pack loader and resolve from the built-ins).
///
/// Built here on the owning thread because the `Renderer` it backs then lives
/// inside the RT callback, with no in-place mutation afterwards — so growing the
/// registry means a fresh stream (see [`Session::set_tracks`]).
fn open_stream(
    cfg: &GroveConfig,
    needed: &HashSet<String>,
) -> Result<(StreamSink, OutputStream), AudioError> {
    let mut registry = Registry::new();
    registry.install_builtin_synths();
    packs::load_subset_into(cfg, &mut registry, needed);
    open_output_stream(Vec::new(), registry)
}

/// The built-in synth instrument names (always resolvable, no pack). Used to seed
/// a session's `loaded` set so synth-only patches don't trigger a registry
/// rebuild. Cheap — in-memory presets, no decode.
fn builtin_synth_names() -> HashSet<String> {
    let mut reg = Registry::new();
    reg.install_builtin_synths();
    reg.instruments_list().into_iter().map(|i| i.name).collect()
}

/// The live audio session: the transport (which owns the stream's command sink),
/// the stream handle keeping cpal alive, the current arrangement (for the hap
/// highlight), and the set of sample instruments already decoded into the
/// registry. The registry lives inside the RT callback and can't be mutated in
/// place, so adding a voice means reopening the stream — the session is the unit
/// we rebuild.
struct Session {
    transport: Transport<StreamSink>,
    /// Keeps the cpal stream alive; dropped (and replaced) on a rebuild and at
    /// session end. `!Send` — lives on this thread only.
    _stream: OutputStream,
    /// The shell's own copy of the live arrangement (the transport doesn't expose
    /// its `Tracks`), for the active-hap highlight.
    current: Tracks<ControlMap>,
    /// Instrument names the live registry already resolves: the built-in synths
    /// (seeded at open) plus every sample voice decoded so far. A referenced name
    /// outside this set is what triggers a rebuild.
    loaded: HashSet<String>,
}

impl Session {
    /// Open a session whose registry holds the built-in synths plus `needed`.
    fn open(cfg: &GroveConfig, needed: &HashSet<String>) -> Result<Session, AudioError> {
        let (sink, stream) = open_stream(cfg, needed)?;
        // Seed `loaded` with the always-present built-in synth names so a
        // synth-only patch never triggers a (pointless) rebuild; pack voices are
        // added to the set as they're first referenced.
        let mut loaded = builtin_synth_names();
        loaded.extend(needed.iter().cloned());
        Ok(Session {
            transport: Transport::new(sink, cfg.default_cps),
            _stream: stream,
            current: Tracks { tracks: Vec::new() },
            loaded,
        })
    }

    /// Apply one control message. Returns `false` on `Shutdown` (caller exits).
    fn apply(&mut self, app: &AppHandle, cfg: &GroveConfig, msg: GroveControl) -> bool {
        match msg {
            GroveControl::SetTracks { tracks, cps, tempo } => {
                self.set_tracks(app, cfg, tracks, cps, tempo)
            }
            GroveControl::Play => self.transport.play(),
            GroveControl::Stop => self.transport.stop(),
            GroveControl::Seek { cycle } => {
                // Quantize the seek target to a whole cycle (the engine's `Time`
                // is rational; sub-cycle seeking is a future refinement).
                self.transport.seek(Time::int(cycle.round() as i64));
            }
            GroveControl::SetCps { cps } => self.transport.set_cps(cps),
            // Live mixer overrides: forwarded straight to the sink (non-blocking;
            // dropping the command on a full queue is acceptable for a knob drag).
            GroveControl::SetTrackGain { track, gain } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackGain(track, gain));
            }
            GroveControl::SetTrackPan { track, pan } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackPan(track, pan));
            }
            GroveControl::SetTrackMute { track, mute } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackMute(track, mute));
            }
            GroveControl::SetTrackSolo { track, solo } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackSolo(track, solo));
            }
            GroveControl::SetMasterGain { gain } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetMasterGain(gain));
            }
            GroveControl::Shutdown => return false,
        }
        true
    }

    /// Stage a new arrangement. If it references a sample instrument not yet
    /// decoded, rebuild the stream first (merging the new voices into the
    /// registry) while preserving the playhead + play state across the swap;
    /// otherwise just push the tracks to the live transport.
    fn set_tracks(
        &mut self,
        app: &AppHandle,
        cfg: &GroveConfig,
        tracks: Tracks<ControlMap>,
        cps: Option<f64>,
        tempo: TempoMap,
    ) {
        self.current = tracks.clone();

        // Lazily decode any newly-referenced instruments. The registry is inside
        // the RT callback, so "load more" means reopening the stream with a wider
        // registry — only when a genuinely new name shows up.
        let referenced = super::validate::referenced_instruments(&tracks);
        let resume = if referenced.is_subset(&self.loaded) {
            None
        } else {
            let needed: HashSet<String> = self.loaded.union(&referenced).cloned().collect();
            let was_playing = self.transport.is_playing();
            let pos = self.transport.position_cycle();
            match open_stream(cfg, &needed) {
                Ok((sink, stream)) => {
                    // Commit the wider set only on success; replace transport +
                    // stream (dropping the old stream stops its audio). tracks /
                    // tempo / cps are re-applied just below.
                    self.loaded = needed;
                    self.transport = Transport::new(sink, cps.unwrap_or(cfg.default_cps));
                    self._stream = stream;
                    Some((was_playing, pos))
                }
                Err(e) => {
                    // Keep the current session (and `loaded`, so it retries next
                    // eval); the new instrument falls back to the synth meanwhile.
                    tracing::warn!("grove: registry rebuild failed ({e}); keeping current voices");
                    emit(app, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
                    None
                }
            }
        };

        // (Re)apply the arrangement to the (possibly fresh) transport.
        self.transport.set_tracks(tracks);
        // A tempo-map drives the clock when present; otherwise the script's
        // constant `cps(...)` applies. Always push the map (empty clears any
        // previous automation back to a constant clock).
        let has_tempo = !tempo.is_empty();
        self.transport.set_tempo_map(tempo);
        if !has_tempo {
            if let Some(c) = cps {
                self.transport.set_cps(c);
            }
        }

        // Restore the playhead + play state lost when the stream was rebuilt.
        if let Some((was_playing, pos)) = resume {
            self.transport.seek(Time::int(pos.round() as i64));
            if was_playing {
                self.transport.play();
            }
        }
    }
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
