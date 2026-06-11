//! The real-time transport: owns the cycle clock and feeds the look-ahead window
//! to an [`AudioSink`].
//!
//! Driven by periodic [`tick`](Transport::tick) calls (~every 25 ms on a worker
//! thread, look-ahead ~100 ms — `architecture.md`). Each tick reads the sink's
//! sample clock ("now"), schedules any cycles newly inside `[now, now+lookahead]`,
//! and pushes their [`VoiceEvent`](arbor_grove_audio::prelude::VoiceEvent)s.
//!
//! Tempo changes and re-eval (a new `Tracks`) are staged as **pending** and
//! applied at the next cycle boundary not yet scheduled — quantized so no event
//! is cut mid-flight and the clock never resets.
//!
//! ## Back-pressure
//!
//! [`AudioSink::send`] is non-blocking and hands the command back on a full queue.
//! When that happens mid-tick the transport **stops pushing for the rest of the
//! tick** and does *not* advance `scheduled_through`, so the same segment is
//! re-scheduled (and re-id'd) on the next tick. The trade-off: a persistently full
//! queue makes the engine re-schedule the same near-future window until it drains,
//! never blocking the worker thread; any events already accepted before the failure
//! in that segment are re-emitted on retry (a rare, degenerate-overload condition,
//! and voices self-release). The sustained dedup set is only *marked* after a
//! successful send, so back-pressure can never silently drop a stem's first onset.
//!
//! ## Sustained dedup
//!
//! A sustained file source (`audio(...)`) is `pure`, so it appears once per cycle
//! forever. [`schedule_span`](crate::schedule::schedule_span) collapses the repeats
//! *within* one call; across ticks the transport keeps a `started` set keyed by
//! `(track, path)` and filters them out. The set is cleared on a track swap and on
//! [`seek`](Transport::seek) — both are points where a stem legitimately restarts.

use std::collections::HashMap;
use std::collections::HashSet;

use arbor_grove_audio::prelude::{AudioCommand, AudioSink, DelayConfig, TrackConfig, VoiceSource};
use arbor_grove_pattern::prelude::{ControlMap, SourceKind, TempoMap, Time, Tracks};

use crate::clock::Epoch;
use crate::schedule::{delay_config_for, schedule_span};

/// Look-ahead window in milliseconds (how far ahead of "now" we schedule).
pub const LOOKAHEAD_MS: u64 = 100;

/// Live scheduler over an audio sink.
#[derive(Debug)]
pub struct Transport<S: AudioSink> {
    sink: S,
    epoch: Epoch,
    tracks: Tracks<ControlMap>,
    next_id: u64,
    /// Highest frame already scheduled (exclusive); the next tick resumes here.
    scheduled_through: u64,
    playing: bool,
    /// Live tempo override (UI `set_cps`), applied at the next unscheduled cycle
    /// boundary. Ignored while a [`TempoMap`] drives the tempo.
    pending_cps: Option<f64>,
    /// Active piecewise-constant tempo automation; empty = constant clock. When
    /// non-empty the transport re-anchors the epoch at each cycle boundary to the
    /// map's `cps`, so the scripted tempo plays itself.
    tempo: TempoMap,
    /// Staged tempo-map (a re-eval), applied at the next unscheduled cycle boundary.
    pending_tempo: Option<TempoMap>,
    /// Staged re-eval, applied at the next unscheduled cycle boundary.
    pending_tracks: Option<Tracks<ControlMap>>,
    /// Cross-tick dedup of sustained stems, keyed by `(track, path)`. A stem
    /// already started in an earlier window is not retriggered. Cleared on swap /
    /// seek (the only points where a stem legitimately restarts).
    sustained_started: HashSet<(u32, String)>,
    /// Last delay-bus config sent per track, so `SetTrackDelay` is re-emitted only
    /// when a track's delay line actually changes. Cleared on swap / seek.
    delay_state: HashMap<u32, DelayConfig>,
}

impl<S: AudioSink> Transport<S> {
    /// Build a stopped transport at `cps` with an empty output.
    pub fn new(sink: S, cps: f64) -> Self {
        Transport {
            sink,
            epoch: Epoch::start(cps),
            tracks: Tracks { tracks: Vec::new() },
            next_id: 0,
            scheduled_through: 0,
            playing: false,
            pending_cps: None,
            tempo: TempoMap::none(),
            pending_tempo: None,
            pending_tracks: None,
            sustained_started: HashSet::new(),
            delay_state: HashMap::new(),
        }
    }

    /// Replace the output (a re-eval). Applied quantized at the next cycle boundary.
    pub fn set_tracks(&mut self, tracks: Tracks<ControlMap>) {
        self.pending_tracks = Some(tracks);
    }

    /// Change tempo (a live UI override). Applied quantized at the next cycle
    /// boundary, re-anchoring the [`Epoch`] so the position stays continuous. A
    /// no-op effect while a [`TempoMap`] is driving the tempo.
    pub fn set_cps(&mut self, cps: f64) {
        self.pending_cps = Some(cps);
    }

    /// Install a piecewise-constant tempo automation (from the script's
    /// `tempo(...)`). An empty map clears automation back to the constant clock.
    /// Applied quantized at the next cycle boundary (like a re-eval).
    pub fn set_tempo_map(&mut self, map: TempoMap) {
        self.pending_tempo = Some(map);
    }

    /// Start scheduling from the sink's current frame.
    pub fn play(&mut self) {
        self.playing = true;
        self.scheduled_through = self.sink.now_frame();
        self.sustained_started.clear();
        self.delay_state.clear();
        // A (re)start promotes a staged tempo-map immediately — the tempo is then
        // anchored at the first scheduled cycle boundary by `tick`/`apply_boundary`.
        if let Some(m) = self.pending_tempo.take() {
            self.tempo = m;
        }
        let _ = self.send_track_config();
    }

    /// Stop and release all voices.
    pub fn stop(&mut self) {
        self.playing = false;
        let _ = self.sink.send(AudioCommand::StopAll);
    }

    /// Jump the cycle clock so cycle `cycle` aligns with the sink's current frame.
    pub fn seek(&mut self, cycle: Time) {
        let now = self.sink.now_frame();
        self.epoch = Epoch {
            frame: now,
            cycle,
            cps: self.epoch.cps,
        };
        self.scheduled_through = now;
        // A seek discontinues the timeline: any sustained stem must be free to
        // restart at its new position, and the delay buses re-arm from scratch.
        self.sustained_started.clear();
        self.delay_state.clear();
    }

    /// Refill the look-ahead window: schedule newly-due cycles and push their
    /// events, applying any pending swap and the tempo automation at cycle
    /// boundaries.
    ///
    /// Walks `[scheduled_through, now + lookahead)` in segments delimited by whole
    /// cycle boundaries. At each boundary it applies a due track/tempo-map swap and
    /// re-anchors the clock to the tempo map's `cps` (a no-op when the tempo is
    /// unchanged), so a scripted `tempo(...)` plays itself and an event never
    /// straddles a tempo change. Stops early (without advancing past the unsent
    /// frame) if the sink queue fills, so the same window is retried next tick.
    pub fn tick(&mut self) {
        if !self.playing {
            return;
        }
        let sr = self.sink.sample_rate();
        let now = self.sink.now_frame();
        let target = now + lookahead_frames(sr);

        // Nothing newly due (clock hasn't advanced past what we already scheduled).
        if target <= self.scheduled_through {
            return;
        }

        while self.scheduled_through < target {
            // If we sit exactly on a cycle boundary, apply swaps + re-anchor tempo
            // there before scheduling the upcoming segment. `scheduled_through` is
            // always a boundary frame or the previous window's target; the *nearest*
            // integer cycle (round, robust to the frame_of rounding for non-integer
            // frames-per-cycle) identifies a real boundary by an exact frame match.
            let nearest = self.epoch.cycle_of(self.scheduled_through, sr).round() as i64;
            if self.epoch.frame_of(Time::int(nearest), sr) == self.scheduled_through {
                self.apply_boundary(nearest, self.scheduled_through);
            }

            // Schedule up to the next whole cycle boundary strictly ahead, or the
            // window target — whichever is first. The epoch may have just been
            // re-anchored, so recompute; the loop guarantees the boundary frame is
            // strictly greater (frame_of can round onto the current frame).
            let pos = self.epoch.cycle_of(self.scheduled_through, sr);
            let mut next_cycle = pos.floor() as i64 + 1;
            let mut next_frame = self.epoch.frame_of(Time::int(next_cycle), sr);
            while next_frame <= self.scheduled_through {
                next_cycle += 1;
                next_frame = self.epoch.frame_of(Time::int(next_cycle), sr);
            }
            let segment_end = next_frame.min(target);

            if !self.schedule_segment(self.scheduled_through..segment_end) {
                // Back-pressure: queue full. Leave `scheduled_through` unmoved so the
                // same window is retried next tick; the swap stays pending.
                return;
            }
            self.scheduled_through = segment_end;
        }
    }

    /// Whether the transport is running.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The current clock anchor.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// Borrow the sink (e.g. to read meters in tests).
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Mutably borrow the sink. The production driver does not need this (it pushes
    /// through [`AudioSink::send`]); it exists for the offline/test sinks whose
    /// clock is advanced manually (`RecordingSink::advance` / `set_now`).
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    // ── Internals ────────────────────────────────────────────────────────────

    /// Apply everything due at the whole-cycle boundary `cycle` (frame `at`):
    /// staged track / tempo-map swaps first, then re-anchor the clock to the tempo
    /// in force. Called once per boundary the scheduler crosses.
    ///
    /// Tempo precedence: a non-empty [`TempoMap`] drives the tempo (re-anchor to
    /// `cps_at(cycle)`, usually a no-op since the `cps` is unchanged mid-segment); a
    /// live `set_cps` override applies only when no map is active. Re-anchoring with
    /// the same `cps` is exact (frame continuous), so per-boundary calls don't drift.
    fn apply_boundary(&mut self, cycle: i64, at: u64) {
        // A staged tempo-map (live re-eval) takes effect here.
        if let Some(m) = self.pending_tempo.take() {
            self.tempo = m;
        }
        if let Some(tracks) = self.pending_tracks.take() {
            self.tracks = tracks;
            // A new track set means new strips and a fresh dedup baseline.
            self.sustained_started.clear();
            self.delay_state.clear();
            let _ = self.send_track_config();
        }
        // The map drives the tempo when present; else a one-shot live override.
        let next_cps = self
            .tempo
            .cps_at(cycle)
            .or_else(|| self.pending_cps.take());
        if let Some(cps) = next_cps {
            if cps != self.epoch.cps {
                self.epoch = self.epoch.reanchor(Time::int(cycle), at, cps);
            }
        }
    }

    /// Schedule and push one contiguous segment `[range.start, range.end)`.
    ///
    /// Returns `false` if the sink queue filled before the segment was fully
    /// pushed (back-pressure) — the caller then leaves `scheduled_through` unmoved.
    /// Sustained stems already started in an earlier window are filtered here.
    fn schedule_segment(&mut self, range: std::ops::Range<u64>) -> bool {
        if range.start >= range.end {
            return true;
        }
        let events = schedule_span(
            &self.tracks,
            &self.epoch,
            self.sink.sample_rate(),
            range,
            &mut self.next_id,
        );
        let sr = self.sink.sample_rate();
        for ev in events {
            // Cross-window sustained dedup: skip a stem already started, but only
            // *mark* it started after a successful send so back-pressure can't drop
            // it (the segment is re-scheduled next tick if send fails).
            let sustained_key = match &ev.source {
                VoiceSource::File {
                    path,
                    kind: SourceKind::Sustained,
                } => {
                    let key = (ev.track, path.clone());
                    if self.sustained_started.contains(&key) {
                        continue;
                    }
                    Some(key)
                }
                _ => None,
            };
            // Reconfigure the track's delay bus only when its config changes; send
            // it ahead of the voice. On back-pressure leave `delay_state` unmarked
            // so the reconfigure is retried with the voice next tick.
            if let Some(AudioCommand::SetTrackDelay(track, cfg)) = delay_config_for(&ev, &self.epoch, sr) {
                if self.delay_state.get(&track) != Some(&cfg) {
                    if self.sink.send(AudioCommand::SetTrackDelay(track, cfg)).is_err() {
                        return false;
                    }
                    self.delay_state.insert(track, cfg);
                }
            }
            if self.sink.send(AudioCommand::Voice(ev)).is_err() {
                return false;
            }
            if let Some(key) = sustained_key {
                self.sustained_started.insert(key);
            }
        }
        true
    }

    /// Tell the mixer about the current strips. Called on `play` and on a swap.
    fn send_track_config(&mut self) -> Result<(), AudioCommand> {
        let cfg = self
            .tracks
            .tracks
            .iter()
            .map(|t| TrackConfig {
                name: t.name.clone(),
            })
            .collect();
        self.sink.send(AudioCommand::ConfigureTracks(cfg))
    }
}

/// Look-ahead distance in frames for `sample_rate`.
fn lookahead_frames(sample_rate: u32) -> u64 {
    (sample_rate as u64 * LOOKAHEAD_MS) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_grove_audio::prelude::RecordingSink;
    use arbor_grove_pattern::prelude::{audio, pure, seq, track, tracks, TempoMap};

    const SR: u32 = 48_000;

    fn drum_tracks(name: &str, sound: &str, hits: usize) -> Tracks<ControlMap> {
        let pat = seq((0..hits).map(|_| pure(ControlMap::sound(sound))).collect());
        tracks(vec![track(name, pat)])
    }

    #[test]
    fn tick_schedules_lookahead_window_only() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 4)); // bd every 1/4 cycle
        tr.play();
        // play() applied no tracks (pending) — but tick at frame 0 applies the swap
        // at the cycle-0 boundary before scheduling.
        tr.tick();
        // Lookahead = 100ms = 4_800 frames → only the onset at frame 0 is in range.
        let voices: Vec<_> = tr.sink().voices().cloned().collect();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].start_frame, 0);
    }

    #[test]
    fn tick_advances_as_clock_moves() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 4));
        tr.play();
        tr.tick(); // schedules [0, 4_800): onset @0
        // Advance ~half a cycle and tick again; onset @12_000 and @24_000 come in
        // range as the window slides.
        // First move to 10_000: window [10_000, 14_800) → onset @12_000.
        tr.sink_mut().set_now(10_000);
        tr.tick();
        let starts: Vec<u64> = tr.sink().voices().map(|v| v.start_frame).collect();
        assert!(starts.contains(&0));
        assert!(starts.contains(&12_000));
        // No duplicates.
        let mut uniq = starts.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), starts.len());
    }

    #[test]
    fn quantized_cps_swap_at_cycle_boundary() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 1)); // one bd per cycle (frame 0, 48_000, …)
        tr.play();
        // Schedule through a few cycles by sweeping the clock.
        for now in [0u64, 40_000, 90_000] {
            tr.sink_mut().set_now(now);
            tr.tick();
        }
        // Now request a tempo double; it must apply at the next unscheduled cycle
        // boundary, re-anchoring the epoch (frame continuous).
        tr.set_cps(2.0);
        tr.sink_mut().set_now(140_000);
        tr.tick();
        let e = tr.epoch();
        assert_eq!(e.cps, 2.0);
        // After the sweep, `scheduled_through` = 94_800; the first cycle boundary
        // at/after that is cycle 2 (frame 96_000 under cps=1). The swap re-anchors
        // there, keeping the frame continuous; frames_per_cycle halves going forward.
        assert_eq!(e.frame, 96_000);
        assert_eq!(e.cycle, Time::int(2));
        // Cycle 3 is now one half-second later: 24_000 frames past the boundary.
        assert_eq!(e.frame_of(Time::int(3), SR), 96_000 + 24_000);
    }

    #[test]
    fn tempo_map_reanchors_at_segment_boundaries() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 1));
        // 1 cps for cycles 0–1, then 2 cps for cycles 2–3, looping every 4 cycles.
        tr.set_tempo_map(TempoMap::from_segments(&[(2, 1.0), (2, 2.0)]));
        tr.play();
        // Sweep the clock across the cycle-2 boundary (frame 96_000 at cps=1).
        for now in [0u64, 48_000, 96_000, 140_000] {
            tr.sink_mut().set_now(now);
            tr.tick();
        }
        let e = tr.epoch();
        // The tempo doubled at cycle 2, anchored at frame 96_000 (frame-continuous).
        assert_eq!(e.cps, 2.0);
        assert_eq!(e.frame, 96_000);
        assert_eq!(e.cycle, Time::int(2));
        // Cycle 3 is now half a cycle-second later: 24_000 frames past the boundary.
        assert_eq!(e.frame_of(Time::int(3), SR), 96_000 + 24_000);
    }

    #[test]
    fn quantized_track_swap_reconfigures_and_resets_dedup() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(tracks(vec![track("stem", audio("a.wav"))]));
        tr.play();
        // Sweep a few cycles: the sustained stem fires exactly once.
        for now in [0u64, 48_000, 96_000] {
            tr.sink_mut().set_now(now);
            tr.tick();
        }
        let sustained_a = tr
            .sink()
            .voices()
            .filter(|v| matches!(&v.source, VoiceSource::File { path, .. } if path.as_str() == "a.wav"))
            .count();
        assert_eq!(sustained_a, 1);

        // Swap to a new stem; it must (re)configure tracks and let the new stem fire.
        tr.set_tracks(tracks(vec![track("stem", audio("b.wav"))]));
        tr.sink_mut().set_now(150_000);
        tr.tick();
        let sustained_b = tr
            .sink()
            .voices()
            .filter(|v| matches!(&v.source, VoiceSource::File { path, .. } if path.as_str() == "b.wav"))
            .count();
        assert_eq!(sustained_b, 1);
        // A ConfigureTracks was sent for the swap (besides the play() one).
        let configs = tr
            .sink()
            .commands()
            .iter()
            .filter(|c| matches!(c, AudioCommand::ConfigureTracks(_)))
            .count();
        assert!(configs >= 2);
    }

    #[test]
    fn stop_releases_all() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 4));
        tr.play();
        tr.tick();
        tr.stop();
        assert!(!tr.is_playing());
        assert!(tr
            .sink()
            .commands()
            .iter()
            .any(|c| matches!(c, AudioCommand::StopAll)));
    }

    /// A sink that accepts at most `voice_cap` **voice** commands before reporting
    /// the queue full (control commands always pass), so the back-pressure path is
    /// exercised deterministically. The cap can be raised to simulate the queue
    /// draining.
    #[derive(Debug)]
    struct CappedSink {
        sent: Vec<AudioCommand>,
        voice_cap: usize,
        voices_sent: usize,
        now: u64,
        sr: u32,
    }

    impl CappedSink {
        fn new(voice_cap: usize, sr: u32) -> Self {
            CappedSink {
                sent: Vec::new(),
                voice_cap,
                voices_sent: 0,
                now: 0,
                sr,
            }
        }
        fn set_cap(&mut self, cap: usize) {
            self.voice_cap = cap;
        }
        fn voice_starts(&self) -> Vec<u64> {
            self.sent
                .iter()
                .filter_map(|c| match c {
                    AudioCommand::Voice(v) => Some(v.start_frame),
                    _ => None,
                })
                .collect()
        }
    }

    impl AudioSink for CappedSink {
        fn send(&mut self, cmd: AudioCommand) -> Result<(), AudioCommand> {
            if matches!(cmd, AudioCommand::Voice(_)) {
                if self.voices_sent >= self.voice_cap {
                    return Err(cmd);
                }
                self.voices_sent += 1;
            }
            self.sent.push(cmd);
            Ok(())
        }
        fn now_frame(&self) -> u64 {
            self.now
        }
        fn sample_rate(&self) -> u32 {
            self.sr
        }
    }

    #[test]
    fn back_pressure_caps_voices_then_retry_recovers() {
        // 16 bd/cycle → onsets every 3_000 frames; lookahead 4_800 → @0 and @3_000
        // fall in the first window. Cap voices at 1: only @0 gets through, @3_000 is
        // refused and the window is left unmoved for retry.
        let mut tr = Transport::new(CappedSink::new(1, SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 16));
        tr.play();
        tr.tick();
        let first = tr.sink().voice_starts();
        assert!(first.contains(&0));
        assert!(!first.contains(&3_000), "back-pressure must refuse the 2nd voice");
        assert!(tr.is_playing());

        // Drain (raise the cap) and tick again at the same clock: the unmoved
        // window is retried and the refused onset now lands.
        tr.sink_mut().set_cap(100);
        tr.tick();
        assert!(
            tr.sink().voice_starts().contains(&3_000),
            "retry must schedule the previously-refused onset"
        );
    }

    #[test]
    fn sweeping_clock_schedules_each_onset_once() {
        // 4 bd/cycle (onsets @0, 12k, 24k, 36k). Sweep the clock across the cycle in
        // sub-lookahead steps; each onset is scheduled exactly once — no duplicate
        // (scheduled_through advances) and no miss (lookahead > step). The sweep
        // stops short of frame 48_000 so cycle 1's onset stays out.
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 4));
        tr.play();
        for now in (0u64..=34_000).step_by(2_000) {
            tr.sink_mut().set_now(now);
            tr.tick();
        }
        let mut starts: Vec<u64> = tr.sink().voices().map(|v| v.start_frame).collect();
        starts.sort_unstable();
        assert_eq!(starts, vec![0, 12_000, 24_000, 36_000]);
    }
}
