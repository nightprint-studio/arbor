//! The dedicated **merula audio thread**.
//!
//! One OS thread (never the job system, never the async runtime — hard rule:
//! the real-time path is sacred) owns the cpal [`OutputStream`] (which is `!Send`
//! and must live and die on the thread that opened it) and runs the look-ahead
//! driver: every ~tick it drains pending control messages, calls
//! [`Transport::tick`], and pushes throttled BE->FE events. The cpal callback runs
//! on its own internal thread; this thread only stays ~100 ms ahead of it.
//!
//! Lifecycle: spawned lazily on the first eval/play, torn down on `Shutdown`
//! (window close) — at which point dropping [`OutputStream`] here stops cpal.
//!
//! Ported from the shell's `src-tauri/src/merula/audio_thread.rs`: the egress
//! changed from the Tauri `AppHandle` to an `Arc<dyn EventSink>` (the shell
//! re-emits each topic to the merula window). The `!Send` stream stays on this
//! dedicated OS thread, unchanged. Speech synthesis lives in the `speech`
//! submodule (the registry builder's only consumer).

mod speech;

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use merula::prelude::{
    open_output_stream, schedule_span, AudioCommand, AudioError, AudioSink, ControlMap, Epoch,
    OutputStream, Registry, ReverbIr, SpeechSpec, StreamSink, TempoMap, Time, TimeSpan, Tracks,
    Transport,
};

use crate::config_cmds::MerulaConfig;
use crate::control::{MerulaControl, Prepared};
use crate::events::{
    emit, ActiveHap, ActiveHaps, AudioErrorEvent, Meters, TransportState, EVT_ACTIVE_HAPS,
    EVT_AUDIO_ERROR, EVT_METERS, EVT_TRANSPORT,
};
use crate::packs;

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
    sink: Arc<dyn EventSink>,
    rx: Receiver<MerulaControl>,
    cfg: MerulaConfig,
    loaded: Arc<Mutex<HashSet<String>>>,
) {
    // Start with synths only (always available, no decode); sample voices arrive
    // pre-decoded from the command on first reference.
    let mut session = match Session::open(&cfg, loaded) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("merula: failed to open audio output: {e}");
            emit(&*sink, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
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
                if !session.apply(&*sink, &cfg, msg) {
                    break;
                }
                while let Ok(msg) = rx.try_recv() {
                    if !session.apply(&*sink, &cfg, msg) {
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
            emit_transport_and_meters(&*sink, &session.transport);
            last_emit = Instant::now();
        }

        // Active-hap highlight: on change only (cheap query each tick).
        let haps = active_haps(&session.transport, &session.current);
        if haps != last_haps {
            emit(&*sink, EVT_ACTIVE_HAPS, ActiveHaps { haps: haps.clone() });
            last_haps = haps;
        }
    }
}

/// Build a sound registry: the built-in synths plus exactly the `needed` sample
/// instruments, **decoded** from their installed packs (the slow part — seconds
/// for a big pack). Called off the RT thread (the command's blocking worker) so
/// the decode never stalls playback; the audio thread only consumes the result.
pub(crate) fn build_registry(
    cfg: &MerulaConfig,
    needed: &HashSet<String>,
    speech: &[SpeechSpec],
) -> Registry {
    let mut registry = Registry::new();
    registry.install_builtin_synths();
    // Global name aliases (`s("kick")` → `RolandTR808_bd`). Install them so the RT
    // resolve maps the alias, AND expand the decode set with each referenced
    // alias's TARGET — `needed` carries the source names (the alias), but the pack
    // manifest only has the target, so without this the target's samples never
    // decode and the alias falls back to the synth.
    let aliases = crate::fstate::load_aliases();
    let mut needed = needed.clone();
    for (alias, target) in &aliases {
        registry.add_alias(alias, target);
        if needed.contains(alias) {
            needed.insert(target.clone());
        }
    }
    packs::load_subset_into(cfg, &mut registry, &needed);
    // Synthesize + register any referenced `speech(...)` sources (off-thread,
    // memoised on disk). The keys are part of `needed`, so a new speech request
    // triggers this rebuild path just like a new sample voice.
    self::speech::register_into(&mut registry, speech, &needed);
    registry
}

/// The built-in synth instrument names (always resolvable, no pack). Used to seed
/// a session's `loaded` set so synth-only patches don't trigger a registry
/// rebuild. Cheap — in-memory presets, no decode.
pub(crate) fn builtin_synth_names() -> HashSet<String> {
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
    /// The live decoded registry (built-in synths + every sample voice decoded so
    /// far) — shares the heavy `Arc<[f32]>` sample data with the stream's copy.
    /// Kept resident so a **device switch** reopens the stream on the same voices
    /// without re-decoding any WAV (the slow part). Replaced on each registry swap.
    registry: Registry,
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
        cfg: &MerulaConfig,
        loaded: Arc<Mutex<HashSet<String>>>,
    ) -> Result<Session, AudioError> {
        let device = cfg.output_device.clone();
        // Build the synths-only registry once and keep it; the stream takes a clone
        // (sharing the Arc sample data), so a later device switch can reopen without
        // a rebuild.
        let registry = build_registry(cfg, &HashSet::new(), &[]);
        let (sink, stream) = open_output_stream(device.as_deref(), Vec::new(), registry.clone())?;
        Ok(Session {
            transport: Transport::new(sink, cfg.default_cps),
            _stream: stream,
            current: Tracks { tracks: Vec::new() },
            loaded,
            registry,
            device,
            tempo: TempoMap::default(),
            audition_id: 0,
        })
    }

    /// Apply one control message. Returns `false` on `Shutdown` (caller exits).
    fn apply(&mut self, sink: &dyn EventSink, cfg: &MerulaConfig, msg: MerulaControl) -> bool {
        match msg {
            MerulaControl::SetTracks { tracks, cps, tempo, prepared } => {
                self.set_tracks(sink, cfg, tracks, cps, tempo, prepared)
            }
            MerulaControl::Play => self.transport.play(),
            MerulaControl::Stop => self.transport.stop(),
            MerulaControl::Seek { cycle } => {
                // Quantize the seek target to a whole cycle (the engine's `Time`
                // is rational; sub-cycle seeking is a future refinement).
                self.transport.seek(Time::int(cycle.round() as i64));
            }
            MerulaControl::SetCps { cps } => self.transport.set_cps(cps),
            MerulaControl::SetOutputDevice { device } => self.set_output_device(sink, device),
            MerulaControl::Audition { tracks, cps, cycles, prepared } => {
                self.audition(sink, tracks, cps, cycles, prepared)
            }
            MerulaControl::StopSnippet => {
                // Clear only the audition bus; the song's voices are untouched.
                let _ = self.transport.sink_mut().send(AudioCommand::StopAudition);
            }
            // Live mixer overrides: forwarded straight to the sink (non-blocking;
            // dropping the command on a full queue is acceptable for a knob drag).
            MerulaControl::SetTrackGain { track, gain } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackGain(track, gain));
            }
            MerulaControl::SetTrackPan { track, pan } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackPan(track, pan));
            }
            MerulaControl::SetTrackMute { track, mute } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackMute(track, mute));
            }
            MerulaControl::SetTrackSolo { track, solo } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetTrackSolo(track, solo));
            }
            MerulaControl::SetMasterGain { gain } => {
                let _ = self.transport.sink_mut().send(AudioCommand::SetMasterGain(gain));
            }
            MerulaControl::SetReverb { seconds } => {
                let _ = self
                    .transport
                    .sink_mut()
                    .send(AudioCommand::SetReverbIr(ReverbIr::Procedural { seconds }));
            }
            MerulaControl::SetMetronome { on } => self.transport.set_metronome(on),
            MerulaControl::SetCountIn { bars } => self.transport.set_count_in_bars(bars),
            MerulaControl::Shutdown => return false,
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
        sink: &dyn EventSink,
        cfg: &MerulaConfig,
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
            Some(prep) => self.swap_registry(sink, prep, cps.unwrap_or(cfg.default_cps)),
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
    fn swap_registry(
        &mut self,
        sink: &dyn EventSink,
        prep: Prepared,
        cps: f64,
    ) -> Option<(bool, f64)> {
        let was_playing = self.transport.is_playing();
        let pos = self.transport.position_cycle();
        let registry = prep.registry;
        match open_output_stream(self.device.as_deref(), Vec::new(), registry.clone()) {
            Ok((sink_handle, stream)) => {
                // Commit the wider set only on success; replace transport + stream
                // (dropping the old stream stops its audio). Keep the decoded registry
                // resident so a later device switch reuses it (no re-decode).
                *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = prep.names;
                self.registry = registry;
                self.transport = Transport::new(sink_handle, cps);
                self._stream = stream;
                Some((was_playing, pos))
            }
            Err(e) => {
                eprintln!("merula: registry rebuild failed ({e}); keeping current voices");
                emit(sink, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
                None
            }
        }
    }

    /// Play a one-off preview / snippet on the dedicated audition bus. When a
    /// referenced instrument was decoded off-thread (`prepared`), swap it in first —
    /// restoring the playing song across the stream swap — then schedule `cycles`
    /// cycles of the evaluated `tracks` (an instrument-preview snippet or an
    /// arbitrary selected chunk) at `cps`, anchored at the current frame, and push
    /// the resulting voices to the preview bus. The renderer routes
    /// [`AudioCommand::Audition`] to its own bus, so the preview is heard cleanly
    /// even while a song plays; each voice self-releases via its duration, so the
    /// one-shot stops on its own. Notes / chords / scales / any effect all come
    /// straight from the evaluated pattern — no per-param plumbing.
    fn audition(
        &mut self,
        sink: &dyn EventSink,
        tracks: Tracks<ControlMap>,
        cps: f64,
        cycles: u32,
        prepared: Option<Prepared>,
    ) {
        if let Some(prep) = prepared {
            let cur_cps = self.transport.epoch().cps;
            if let Some((was_playing, pos)) = self.swap_registry(sink, prep, cur_cps) {
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
        // Schedule cycles [0, cycles) of the snippet, with cycle 0 anchored at `now`.
        let epoch = Epoch { frame: now, cycle: Time::ZERO, cps };
        let fpc = (sr as f64 / cps).max(1.0) as u64;
        let total = fpc.saturating_mul(cycles.max(1) as u64);
        let events = schedule_span(&tracks, &epoch, sr, now..now + total, &mut self.audition_id);
        let sink_mut = self.transport.sink_mut();
        for ev in events {
            let _ = sink_mut.send(AudioCommand::Audition(ev));
        }
    }

    /// Switch the output device live: reopen the stream on the new device reusing
    /// the **already-decoded** registry, preserving the playhead + play state + the
    /// staged arrangement. A no-op when the device is unchanged; on failure it keeps
    /// the current stream and surfaces the error (the old device stays).
    fn set_output_device(&mut self, sink: &dyn EventSink, device: Option<String>) {
        if device == self.device {
            return;
        }
        let was_playing = self.transport.is_playing();
        let pos = self.transport.position_cycle();
        let cps = self.transport.epoch().cps;
        // Reuse the resident registry (cloning shares the Arc sample data) — the
        // voices are unchanged, only the device differs, so NO WAV is re-decoded.
        // The stream still has to reopen on the new device (audio briefly blips),
        // but the previously-slow re-decode of every loaded sample is gone.
        match open_output_stream(device.as_deref(), Vec::new(), self.registry.clone()) {
            Ok((sink_handle, stream)) => {
                self.device = device;
                self.transport = Transport::new(sink_handle, cps);
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
                eprintln!("merula: output-device switch failed ({e}); keeping current device");
                emit(sink, EVT_AUDIO_ERROR, AudioErrorEvent { message: e.to_string() });
            }
        }
    }
}

/// Emit the current transport position and the audio telemetry (master +
/// per-track peak, voice count, DSP load).
fn emit_transport_and_meters(sink: &dyn EventSink, transport: &Transport<StreamSink>) {
    let stream_sink = transport.sink();
    let now = stream_sink.now_frame();
    let sr = stream_sink.sample_rate();
    let epoch = transport.epoch();

    emit(
        sink,
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

    let m = stream_sink.meters();
    emit(
        sink,
        EVT_METERS,
        Meters {
            master: m.master,
            tracks: m.tracks,
            voices: m.voices,
            dsp_load: m.dsp_load,
            gain_reduction: m.gain_reduction,
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
