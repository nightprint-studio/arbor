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

use std::collections::HashSet;

use arbor_grove_audio::prelude::{AudioCommand, AudioSink, TrackConfig, VoiceSource};
use arbor_grove_pattern::prelude::{ControlMap, SourceKind, Time, Tracks};

use crate::clock::Epoch;
use crate::schedule::schedule_span;

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
    /// Staged tempo change, applied at the next unscheduled cycle boundary.
    pending_cps: Option<f64>,
    /// Staged re-eval, applied at the next unscheduled cycle boundary.
    pending_tracks: Option<Tracks<ControlMap>>,
    /// Cross-tick dedup of sustained stems, keyed by `(track, path)`. A stem
    /// already started in an earlier window is not retriggered. Cleared on swap /
    /// seek (the only points where a stem legitimately restarts).
    sustained_started: HashSet<(u32, String)>,
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
            pending_tracks: None,
            sustained_started: HashSet::new(),
        }
    }

    /// Replace the output (a re-eval). Applied quantized at the next cycle boundary.
    pub fn set_tracks(&mut self, tracks: Tracks<ControlMap>) {
        self.pending_tracks = Some(tracks);
    }

    /// Change tempo. Applied quantized at the next cycle boundary, re-anchoring
    /// the [`Epoch`] so the position stays continuous.
    pub fn set_cps(&mut self, cps: f64) {
        self.pending_cps = Some(cps);
    }

    /// Start scheduling from the sink's current frame.
    pub fn play(&mut self) {
        self.playing = true;
        self.scheduled_through = self.sink.now_frame();
        self.sustained_started.clear();
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
        // restart at its new position.
        self.sustained_started.clear();
    }

    /// Refill the look-ahead window: schedule newly-due cycles and push their
    /// events, applying any pending tempo/track swap at the boundary.
    ///
    /// Schedules `[scheduled_through, now + lookahead)`, but only up to the next
    /// unscheduled cycle boundary when a swap is pending — the swap is then applied
    /// (re-anchoring the clock for a tempo change, re-`ConfigureTracks` for a track
    /// change) and the remainder of the window is scheduled under the new state.
    /// Stops early (without advancing past the unsent frame) if the sink queue
    /// fills, so the same window is retried next tick.
    pub fn tick(&mut self) {
        if !self.playing {
            return;
        }
        let now = self.sink.now_frame();
        let lookahead_frames = lookahead_frames(self.sink.sample_rate());
        let target = now + lookahead_frames;

        // Nothing newly due (clock hasn't advanced past what we already scheduled).
        if target <= self.scheduled_through {
            return;
        }

        // Walk the window in segments delimited by cycle boundaries where a pending
        // swap is due. A swap applies at the first cycle boundary at/after
        // `scheduled_through`; if `scheduled_through` already sits exactly on a
        // boundary, the swap takes effect *there*, before that segment is scheduled.
        while self.scheduled_through < target {
            // Apply a swap that is due exactly at the current (already-on-boundary)
            // frame before scheduling the upcoming segment.
            if self.has_pending() {
                if let Some(b) = self.next_swap_boundary_frame() {
                    if b == self.scheduled_through {
                        self.apply_pending_at(b);
                    }
                }
            }

            // Schedule up to the next pending-swap boundary inside the window, else
            // the full target.
            let segment_end = match (self.has_pending(), self.next_swap_boundary_frame()) {
                (true, Some(b)) if b > self.scheduled_through && b < target => b,
                _ => target,
            };

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

    /// Whether any quantized swap is staged.
    fn has_pending(&self) -> bool {
        self.pending_cps.is_some() || self.pending_tracks.is_some()
    }

    /// Absolute frame of the first integer cycle boundary at or after
    /// `scheduled_through` — the earliest point a staged swap may take effect
    /// without cutting an already-scheduled event. If `scheduled_through` sits
    /// exactly on a boundary, that boundary is returned (the swap applies there).
    fn next_swap_boundary_frame(&self) -> Option<u64> {
        let sr = self.sink.sample_rate();
        let pos = self.epoch.cycle_of(self.scheduled_through, sr);
        // `ceil` rounds a fractional position up to the next whole cycle; an exact
        // integer boundary maps to itself. A tiny epsilon absorbs float slop so a
        // boundary we are exactly on is recognised rather than skipped to the next.
        let next_cycle = (pos - 1e-9).ceil() as i64;
        let boundary = Time::int(next_cycle);
        Some(self.epoch.frame_of(boundary, sr))
    }

    /// Apply the staged tempo / track swap at boundary frame `at`.
    fn apply_pending_at(&mut self, at: u64) {
        let sr = self.sink.sample_rate();
        // The cycle that `at` represents under the *current* epoch.
        let boundary_cycle = Time::int(self.epoch.cycle_of(at, sr).round() as i64);

        if let Some(cps) = self.pending_cps.take() {
            self.epoch = self.epoch.reanchor(boundary_cycle, at, cps);
        }
        if let Some(tracks) = self.pending_tracks.take() {
            self.tracks = tracks;
            // A new track set means new strips and a fresh dedup baseline.
            self.sustained_started.clear();
            let _ = self.send_track_config();
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
    use arbor_grove_pattern::prelude::{audio, pure, seq, track, tracks};

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
