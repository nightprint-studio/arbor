//! The real-time transport: owns the cycle clock and feeds the look-ahead window
//! to an [`AudioSink`].
//!
//! Driven by periodic [`tick`](Transport::tick) calls (~every 25 ms on a worker
//! thread, look-ahead ~100 ms — `architecture.md`). Each tick reads the sink's
//! sample clock ("now"), schedules any cycles newly inside `[now, now+lookahead]`,
//! and pushes their [`VoiceEvent`](merula_audio::prelude::VoiceEvent)s.
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

use merula_audio::prelude::{
    AudioCommand, AudioSink, DelayConfig, TrackConfig, VoiceEvent, VoiceId, VoiceParams, VoiceSource,
};
use merula_pattern::prelude::{ControlMap, SourceKind, TempoMap, Time, Tracks};

use crate::clock::Epoch;
use crate::schedule::{delay_config_for, schedule_span, track_fx_commands};

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
    /// Reported playhead cycle while **stopped**. The sink's sample clock keeps
    /// free-running after [`stop`](Self::stop), so reading the live position would
    /// make the UI ruler crawl on past a stop; instead [`position_cycle`] reports
    /// this frozen value when not playing. Updated on stop (freeze where we are)
    /// and seek (jump to the target), and used by [`play`](Self::play) to resume.
    paused_cycle: f64,
    /// Audible click track: when on, [`tick`](Self::tick) emits a metronome click on
    /// every beat (quarter-cycle), accented on the bar (whole cycle). Clicks ride the
    /// **audition bus** so they bypass the song's strips (mute / solo never silence
    /// them). Off by default; a pure monitoring aid, never part of the render.
    metronome: bool,
    /// Count-in length in whole bars (cycles). When `> 0`, [`play`](Self::play) delays
    /// the song by this many cycles: the clock is anchored so the resume position lands
    /// after the pre-roll, the metronome clicks through the pre-roll (forced on, beats
    /// leading into bar 1), and the song's own voices are suppressed until then. `0`
    /// (default) = no pre-roll. Session-only, like the metronome toggle.
    count_in_bars: u32,
    /// Frame at which the song begins after a count-in; `Some` only while a pre-roll is
    /// in flight. Below this frame the scheduler emits clicks but no song voices; it is
    /// cleared once the clock crosses it (and on stop / seek, which discontinue it).
    preroll_end: Option<u64>,
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
            paused_cycle: 0.0,
            metronome: false,
            count_in_bars: 0,
            preroll_end: None,
        }
    }

    /// Enable / disable the audible click track (the metronome).
    pub fn set_metronome(&mut self, on: bool) {
        self.metronome = on;
    }

    /// Set the count-in length in whole bars (`0` = off). Takes effect on the next
    /// [`play`](Self::play); a pre-roll already in flight is unaffected.
    pub fn set_count_in_bars(&mut self, bars: u32) {
        self.count_in_bars = bars;
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

    /// Start scheduling, resuming from the frozen playhead (`paused_cycle`).
    ///
    /// The sink's sample clock free-runs even while stopped, so we re-anchor the
    /// [`Epoch`] here: `paused_cycle` is pinned to the current frame, keeping the
    /// position continuous across a stop/play (and starting at cycle 0 on the very
    /// first play, regardless of how long the device has been open).
    pub fn play(&mut self) {
        let sr = self.sink.sample_rate();
        let now = self.sink.now_frame();
        let fpc = self.epoch.frames_per_cycle(sr);
        // Count-in: shift the resume position forward by N bars, so `paused_cycle`
        // lands at the end of the pre-roll instead of at `now`. The bars in between
        // (negative cycles relative to the song) are metronome-only; the song's own
        // voices below `preroll_end` are suppressed by `schedule_segment`.
        let preroll_frames = self.count_in_bars as f64 * fpc;
        let delta = self.paused_cycle - self.epoch.cycle.to_f64();
        let anchor = (now as f64 + preroll_frames - delta * fpc).round();
        self.epoch.frame = if anchor <= 0.0 { 0 } else { anchor as u64 };
        self.preroll_end = (self.count_in_bars > 0).then(|| now + preroll_frames.round() as u64);
        self.playing = true;
        self.scheduled_through = now;
        self.sustained_started.clear();
        self.delay_state.clear();
        // A (re)start promotes a staged tempo-map immediately — the tempo is then
        // anchored at the first scheduled cycle boundary by `tick`/`apply_boundary`.
        if let Some(m) = self.pending_tempo.take() {
            self.tempo = m;
        }
        let _ = self.send_track_config();
    }

    /// Stop playback: clear every voice and flush the effect tails (via
    /// [`AudioCommand::StopAll`]) so the output returns to exact silence and the
    /// DSP goes idle. Freezes the reported playhead where it is, so the UI ruler
    /// holds still instead of tracking the free-running sink clock.
    pub fn stop(&mut self) {
        self.paused_cycle = self
            .epoch
            .cycle_of(self.sink.now_frame(), self.sink.sample_rate());
        self.playing = false;
        self.preroll_end = None;
        let _ = self.sink.send(AudioCommand::StopAll);
    }

    /// The playhead cycle to report to the UI: the live clock position while
    /// playing, the frozen [`paused_cycle`](Self::paused_cycle) while stopped.
    pub fn position_cycle(&self) -> f64 {
        if self.playing {
            self.epoch
                .cycle_of(self.sink.now_frame(), self.sink.sample_rate())
        } else {
            self.paused_cycle
        }
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
        // Jump the frozen playhead too, so a seek while stopped moves the ruler.
        self.paused_cycle = cycle.to_f64();
        // A seek discontinues the timeline: any sustained stem must be free to
        // restart at its new position, the delay buses re-arm from scratch, and any
        // in-flight count-in pre-roll is abandoned (the new position plays at once).
        self.sustained_started.clear();
        self.delay_state.clear();
        self.preroll_end = None;
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

            let seg_start = self.scheduled_through;
            if !self.schedule_segment(seg_start..segment_end) {
                // Back-pressure: queue full. Leave `scheduled_through` unmoved so the
                // same window is retried next tick; the swap stays pending.
                return;
            }
            self.scheduled_through = segment_end;
            // Metronome clicks for the segment we just committed (best-effort, so a
            // dropped click never blocks the song). The range only advances on a
            // successful song segment, so clicks are never double-emitted. Clicks fire
            // when the toggle is on *or* the segment sits in a count-in pre-roll.
            if self.metronome || self.preroll_end.is_some() {
                self.schedule_metronome(seg_start..segment_end);
            }
            // The pre-roll ends once the clock crosses its boundary; from here the
            // song's voices flow and the click follows only the metronome toggle.
            if let Some(end) = self.preroll_end {
                if self.scheduled_through >= end {
                    self.preroll_end = None;
                }
            }
        }
    }

    /// Emit a metronome click on every beat (quarter-cycle) whose onset frame lands
    /// in `range`, accented on the bar (whole cycle). Clicks go to the audition bus
    /// (bypassing the song strips). Best-effort: a dropped click is acceptable.
    fn schedule_metronome(&mut self, range: std::ops::Range<u64>) {
        let sr = self.sink.sample_rate();
        let fpc = self.epoch.frames_per_cycle(sr);
        let dur = (0.04 * fpc).round().max(1.0) as u64;
        // First beat index (quarter-cycle) at or after the range start.
        let start_cycle = self.epoch.cycle_of(range.start, sr);
        let mut k = (start_cycle * 4.0).ceil() as i64;
        loop {
            let frame = self.epoch.frame_of(Time::new(k, 4), sr);
            if frame >= range.end {
                break;
            }
            // A beat sounds when the metronome is on, or while it still falls inside a
            // count-in pre-roll (so the click count leads cleanly into the first bar).
            let in_preroll = self.preroll_end.is_some_and(|e| frame < e);
            if frame >= range.start && (self.metronome || in_preroll) {
                let accent = k.rem_euclid(4) == 0; // bar start (whole cycle)
                let ev = self.click_event(frame, dur, accent);
                let _ = self.sink.send(AudioCommand::Audition(ev));
            }
            k += 1;
        }
    }

    /// Build one metronome click voice (a short `synth.hat` noise tick on the
    /// audition bus; the accent is louder). `track` is ignored on the audition bus.
    fn click_event(&mut self, start_frame: u64, dur: u64, accent: bool) -> VoiceEvent {
        let id = self.next_id;
        self.next_id += 1;
        let params = VoiceParams {
            gain: if accent { 1.0 } else { 0.45 },
            ..Default::default()
        };
        VoiceEvent {
            id: VoiceId(id),
            start_frame,
            dur_frames: Some(dur),
            legato: false,
            source: VoiceSource::Named {
                sound: None,
                variant: None,
                inst: Some("synth.hat".to_string()),
                art: None,
            },
            // `synth.hat` is noise (pitch-agnostic), but give the voice a defined note
            // so an oscillator-based source still triggers cleanly.
            note: Some(72.0),
            params,
            track: 0,
            span: None,
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
            // Count-in pre-roll: the bars before `preroll_end` are metronome-only, so
            // drop the song's own voices there (without marking sustained / delay state,
            // leaving the first real onset at the boundary free to fire).
            if self.preroll_end.is_some_and(|end| ev.start_frame < end) {
                continue;
            }
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
    fn send_track_config(&mut self) -> Result<(), Box<AudioCommand>> {
        let cfg = self
            .tracks
            .tracks
            .iter()
            .map(|t| TrackConfig {
                name: t.name.clone(),
            })
            .collect();
        self.sink.send(AudioCommand::ConfigureTracks(cfg))?;
        // Per-track FX inserts (parametric EQ + compressor) implied by the source.
        // Strip-level and constant per track, so they ride the (re)configure rather
        // than the per-onset path. Best-effort: at a swap boundary the queue is
        // essentially empty, and the next eval re-sends them anyway.
        for cmd in track_fx_commands(&self.tracks) {
            let _ = self.sink.send(cmd);
        }
        Ok(())
    }
}

/// Look-ahead distance in frames for `sample_rate`.
fn lookahead_frames(sample_rate: u32) -> u64 {
    (sample_rate as u64 * LOOKAHEAD_MS) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    use merula_audio::prelude::RecordingSink;
    use merula_pattern::prelude::{audio, pure, seq, track, tracks, TempoMap};

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
        // 1 cps for cycles 0–1, then 2 cps for cycles 2–7 (period 8). The high-tempo
        // segment is long enough that the swept look-ahead window stays inside it —
        // otherwise the map's loop wrap at the period boundary would (correctly)
        // re-anchor back to 1 cps, which is a separate behaviour from the
        // segment-boundary doubling this test pins down.
        tr.set_tempo_map(TempoMap::from_segments(&[(2, 1.0), (6, 2.0)]));
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
    fn count_in_delays_song_and_clicks_through_preroll() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 1)); // one bd per cycle (frame 0, 48_000, …)
        tr.set_count_in_bars(2); // 2-bar pre-roll = 96_000 frames at cps=1
        tr.play();
        // Sweep across the pre-roll and into bar 1.
        for now in (0u64..=100_000).step_by(4_000) {
            tr.sink_mut().set_now(now);
            tr.tick();
        }
        // The song's bd is suppressed for the whole pre-roll; the first real onset
        // lands at the boundary (frame 96_000), never at frame 0.
        let song_starts: Vec<u64> = tr.sink().voices().map(|v| v.start_frame).collect();
        assert!(!song_starts.contains(&0), "song must stay silent during the count-in");
        assert!(
            song_starts.contains(&96_000),
            "song must start at the pre-roll boundary"
        );
        // The 2 bars of count-in click every beat (8 audition clicks), even though the
        // metronome toggle is off; clicks stop once the pre-roll ends.
        let clicks = tr
            .sink()
            .commands()
            .iter()
            .filter(|c| matches!(c, AudioCommand::Audition(_)))
            .count();
        assert_eq!(clicks, 8, "a 2-bar count-in must click 8 beats then stop");
    }

    #[test]
    fn stop_sends_stop_all() {
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

    #[test]
    fn stop_freezes_reported_position() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 4));
        tr.play();
        tr.sink_mut().set_now(24_000); // half a cycle in (cps=1 → 48_000 frames/cycle)
        tr.tick();
        let at_stop = tr.position_cycle();
        assert!((at_stop - 0.5).abs() < 1e-6, "playing position should track the clock");
        tr.stop();
        // The sink clock keeps free-running after stop…
        tr.sink_mut().set_now(72_000);
        // …but the reported playhead must hold where it stopped.
        assert!(
            (tr.position_cycle() - at_stop).abs() < 1e-6,
            "stopped playhead must freeze, not crawl with the free-running clock"
        );
    }

    #[test]
    fn play_resumes_from_frozen_position() {
        let mut tr = Transport::new(RecordingSink::new(SR), 1.0);
        tr.set_tracks(drum_tracks("d", "bd", 4));
        tr.play();
        tr.sink_mut().set_now(24_000);
        tr.tick();
        tr.stop();
        let frozen = tr.position_cycle();
        // Clock free-runs a full cycle while stopped, then we hit play again.
        tr.sink_mut().set_now(72_000);
        tr.play();
        assert!(
            (tr.position_cycle() - frozen).abs() < 1e-6,
            "play must resume from the frozen playhead, not the drifted clock"
        );
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
        fn send(&mut self, cmd: AudioCommand) -> Result<(), Box<AudioCommand>> {
            if matches!(cmd, AudioCommand::Voice(_)) {
                if self.voices_sent >= self.voice_cap {
                    return Err(Box::new(cmd));
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
