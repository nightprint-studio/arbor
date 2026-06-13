//! The dedicated **nemus audio thread**.
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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use arbor_nemus::prelude::{
    open_output_stream, schedule_span, AudioCommand, AudioError, AudioSink, ControlMap, Epoch,
    OutputStream, Registry, StreamSink, TempoMap, Time, TimeSpan, Tracks, Transport,
};

use super::config::NemusConfig;
use super::control::{NemusControl, Prepared};
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
/// stream with the built-in synths only; sample voices are decoded **off this
/// thread** by the command layer and arrive ready in `SetTracks` (the audio
/// thread only reopens the stream to merge them in). Loops on the control
/// channel; returns when the session ends and the caller's `JoinHandle` resolves.
///
/// Why lazy: a pack like VSCO or Dirt-Samples is gigabytes of audio. Decoding
/// every installed pack up front made the first play stall and held all of it in
/// RAM even for a one-drum patch — now only the referenced instruments load.
///
/// Why off-thread: the decode (`packs::load_subset_into`) can run for seconds; on
/// this driver loop it would starve [`Transport::tick`] and freeze playback while
/// the user edits. So the command pre-decodes on a blocking worker and hands us a
/// ready [`Registry`] via [`Prepared`]; `loaded` is the set those decodes have
/// landed, shared with the command so it knows when a new voice needs building.
pub fn run(
    app: AppHandle,
    rx: Receiver<NemusControl>,
    cfg: NemusConfig,
    loaded: Arc<Mutex<HashSet<String>>>,
) {
    // Start with synths only (always available, no decode); sample voices arrive
    // pre-decoded from the command on first reference.
    let mut session = match Session::open(&cfg, loaded) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("nemus: failed to open audio output: {e}");
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
    cfg: &NemusConfig,
    needed: &HashSet<String>,
    device: Option<&str>,
) -> Result<(StreamSink, OutputStream), AudioError> {
    open_output_stream(device, Vec::new(), build_registry(cfg, needed))
}

/// Build a sound registry: the built-in synths plus exactly the `needed` sample
/// instruments, **decoded** from their installed packs (the slow part — seconds
/// for a big pack). Called off the RT thread (the command's blocking worker) so
/// the decode never stalls playback; the audio thread only consumes the result.
pub(super) fn build_registry(cfg: &NemusConfig, needed: &HashSet<String>) -> Registry {
    let mut registry = Registry::new();
    registry.install_builtin_synths();
    packs::load_subset_into(cfg, &mut registry, needed);
    registry
}

/// The built-in synth instrument names (always resolvable, no pack). Used to seed
/// a session's `loaded` set so synth-only patches don't trigger a registry
/// rebuild. Cheap — in-memory presets, no decode.
pub(super) fn builtin_synth_names() -> HashSet<String> {
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
    /// outside this set is what triggers a rebuild. **Shared** with the command
    /// (`Arc<Mutex>`): the command reads it to decide whether an eval needs a new
    /// (off-thread) decode; the audio thread writes it on a successful swap.
    loaded: Arc<Mutex<HashSet<String>>>,
    /// The chosen cpal output device name (`None` = host default). Carried so a
    /// registry rebuild reopens on the SAME device, and a device switch can reopen
    /// with the current registry.
    device: Option<String>,
    /// The live tempo map (empty = constant clock), kept so reopening the stream
    /// (registry rebuild / device switch) re-applies the same tempo automation.
    tempo: TempoMap,
    /// Monotonic id source for preview voices, bumped by `schedule_span`. The
    /// preview voices live in the renderer's separate audition pool, so these never
    /// collide with the engine's song-voice ids.
    audition_id: u64,
}

impl Session {
    /// Open a session with the built-in synths only (no decode). `loaded` is the
    /// shared set the command already seeded with the built-in synth names; pack
    /// voices are added to it as they're first referenced (decoded off-thread).
    fn open(
        cfg: &NemusConfig,
        loaded: Arc<Mutex<HashSet<String>>>,
    ) -> Result<Session, AudioError> {
        let device = cfg.output_device.clone();
        let (sink, stream) = open_stream(cfg, &HashSet::new(), device.as_deref())?;
        Ok(Session {
            transport: Transport::new(sink, cfg.default_cps),
            _stream: stream,
            current: Tracks { tracks: Vec::new() },
            loaded,
            device,
            tempo: TempoMap::default(),
            audition_id: 0,
        })
    }

    /// Apply one control message. Returns `false` on `Shutdown` (caller exits).
    fn apply(&mut self, app: &AppHandle, cfg: &NemusConfig, msg: NemusControl) -> bool {
        match msg {
            NemusControl::SetTracks { tracks, cps, tempo, prepared } => {
                self.set_tracks(app, cfg, tracks, cps, tempo, prepared)
            }
            NemusControl::Play => self.transport.play(),
            NemusControl::Stop => self.transport.stop(),
            NemusControl::Seek { cycle } => {
                // Quantize the seek target to a whole cycle (the engine's `Time`
                // is rational; sub-cycle seeking is a future refinement).
                self.transport.seek(Time::int(cycle.round() as i64));
            }
            NemusControl::SetCps { cps } => self.transport.set_cps(cps),
            NemusControl::SetOutputDevice { device } => self.set_output_device(app, cfg, device),
            NemusControl::Audition { tracks, cps, prepared } => {
                self.audition(app, tracks, cps, prepared)
            }
            // Live mixer overrides: forwarded straight to the sink (non-blocking;
            // dropping the command on a full queue is acceptable for a knob drag).
            NemusControl::SetTrackGain { track, gain } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackGain(track, gain));
            }
            NemusControl::SetTrackPan { track, pan } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackPan(track, pan));
            }
            NemusControl::SetTrackMute { track, mute } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackMute(track, mute));
            }
            NemusControl::SetTrackSolo { track, solo } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackSolo(track, solo));
            }
            NemusControl::SetMasterGain { gain } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetMasterGain(gain));
            }
            NemusControl::Shutdown => return false,
        }
        true
    }

    /// Stage a new arrangement. When the command pre-decoded new voices (it hands
    /// us a ready [`Prepared`] registry), reopen the stream to merge them in while
    /// preserving the playhead + play state across the swap — **no decode runs on
    /// this RT thread**. Otherwise (`prepared` is `None`: no new voices) just push
    /// the tracks to the live transport.
    fn set_tracks(
        &mut self,
        app: &AppHandle,
        cfg: &NemusConfig,
        tracks: Tracks<ControlMap>,
        cps: Option<f64>,
        tempo: TempoMap,
        prepared: Option<Prepared>,
    ) {
        self.current = tracks.clone();

        // The registry lives inside the RT callback, so "load more" means reopening
        // the stream with a wider registry. The decode already ran off-thread, so
        // here we only swap in the ready registry — fast, no RT stall.
        let resume = match prepared {
            Some(prep) => self.swap_registry(app, prep, cps.unwrap_or(cfg.default_cps)),
            None => None,
        };

        // (Re)apply the arrangement to the (possibly fresh) transport.
        self.transport.set_tracks(tracks);
        // A tempo-map drives the clock when present; otherwise the script's
        // constant `cps(...)` applies. Always push the map (empty clears any
        // previous automation back to a constant clock). Keep a copy so reopening
        // the stream (device switch) re-applies the same automation.
        self.tempo = tempo.clone();
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

    /// Reopen the cpal stream on a registry the command pre-decoded off-thread
    /// ([`Prepared`]) — the only RT work a "load more" costs now. Commits the wider
    /// `loaded` set on success and returns `(was_playing, position_cycle)` so the
    /// caller can restore the playhead after re-applying its arrangement. Keeps the
    /// current stream and returns `None` on failure.
    fn swap_registry(&mut self, app: &AppHandle, prep: Prepared, cps: f64) -> Option<(bool, f64)> {
        let was_playing = self.transport.is_playing();
        let pos = self.transport.position_cycle();
        match open_output_stream(self.device.as_deref(), Vec::new(), prep.registry) {
            Ok((sink, stream)) => {
                // Commit the wider set only on success; replace transport + stream
                // (dropping the old stream stops its audio).
                *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = prep.names;
                self.transport = Transport::new(sink, cps);
                self._stream = stream;
                Some((was_playing, pos))
            }
            Err(e) => {
                tracing::warn!("nemus: registry rebuild failed ({e}); keeping current voices");
                emit(app, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
                None
            }
        }
    }

    /// Play a one-off instrument preview on the dedicated audition bus. When a
    /// referenced instrument was decoded off-thread (`prepared`), swap it in first —
    /// restoring the playing song across the stream swap — then schedule **one
    /// cycle** of the evaluated `tracks` (a generated snippet) at `cps`, anchored at
    /// the current frame, and push the resulting voices to the preview bus. The
    /// renderer routes [`AudioCommand::Audition`] to its own bus, so the preview is
    /// heard cleanly even while a song plays. Notes / chords / scales / any effect
    /// all come straight from the evaluated pattern — no per-param plumbing.
    fn audition(
        &mut self,
        app: &AppHandle,
        tracks: Tracks<ControlMap>,
        cps: f64,
        prepared: Option<Prepared>,
    ) {
        if let Some(prep) = prepared {
            let cur_cps = self.transport.epoch().cps;
            if let Some((was_playing, pos)) = self.swap_registry(app, prep, cur_cps) {
                // Re-apply the live song to the fresh transport so it keeps playing
                // through the preview-driven stream swap.
                self.transport.set_tracks(self.current.clone());
                self.transport.set_tempo_map(self.tempo.clone());
                self.transport.seek(Time::int(pos.round() as i64));
                if was_playing {
                    self.transport.play();
                }
            }
        }

        let (now, sr) = {
            let sink = self.transport.sink();
            (sink.now_frame(), sink.sample_rate())
        };
        // Schedule cycle [0, 1) of the snippet, with cycle 0 anchored at `now`.
        let epoch = Epoch { frame: now, cycle: Time::ZERO, cps };
        let fpc = (sr as f64 / cps).max(1.0) as u64;
        let events = schedule_span(&tracks, &epoch, sr, now..now + fpc, &mut self.audition_id);
        let sink = self.transport.sink_mut();
        for ev in events {
            let _ = sink.send(AudioCommand::Audition(ev));
        }
    }

    /// Switch the output device live: reopen the stream on the new device with the
    /// current registry (the `loaded` set), preserving the playhead + play state +
    /// the staged arrangement. A no-op when the device is unchanged; on failure it
    /// keeps the current stream and surfaces the error (the old device stays).
    fn set_output_device(&mut self, app: &AppHandle, cfg: &NemusConfig, device: Option<String>) {
        if device == self.device {
            return;
        }
        let was_playing = self.transport.is_playing();
        let pos = self.transport.position_cycle();
        let cps = self.transport.epoch().cps;
        // A device switch must rebuild the registry for the new stream; the voices
        // are the same, so we re-decode the current `loaded` set. This is the one
        // place a decode still runs on this thread — but it's rare and explicitly
        // user-initiated (the device is changing, audio necessarily blips), so the
        // brief stall is acceptable.
        let needed = self.loaded.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match open_stream(cfg, &needed, device.as_deref()) {
            Ok((sink, stream)) => {
                self.device = device;
                self.transport = Transport::new(sink, cps);
                self._stream = stream;
                // Re-apply the live arrangement + tempo to the fresh transport,
                // then restore the playhead + play state.
                self.transport.set_tracks(self.current.clone());
                self.transport.set_tempo_map(self.tempo.clone());
                self.transport.seek(Time::int(pos.round() as i64));
                if was_playing {
                    self.transport.play();
                }
            }
            Err(e) => {
                tracing::warn!("nemus: output-device switch failed ({e}); keeping current device");
                emit(app, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
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
