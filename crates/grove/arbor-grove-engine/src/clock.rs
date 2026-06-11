//! The cycle clock: maps grove's exact cycle-time to absolute output frames.
//!
//! The **audio backend owns the sample clock** (the running frame counter); the
//! engine owns **`cps`** and the cycle↔frame mapping. The mapping is anchored at
//! an [`Epoch`] so that a live tempo change (or a re-eval) never retroactively
//! shifts already-scheduled events and the transport position stays continuous
//! (`design/grove/semantics.md`: the clock does not reset).
//!
//! `frames_per_cycle = sample_rate / cps`, and
//! `frame_of(c) = epoch.frame + (c − epoch.cycle) · frames_per_cycle`.

use arbor_grove_pattern::prelude::Time;

/// Anchors cycle-time to the sample timeline: at `frame`, the cycle position is
/// `cycle`, advancing at `cps` cycles per second.
#[derive(Clone, Copy, Debug)]
pub struct Epoch {
    /// Absolute output frame of the anchor.
    pub frame: u64,
    /// Cycle-time at `frame` (usually an integer cycle boundary).
    pub cycle: Time,
    /// Cycles per second in force from this anchor.
    pub cps: f64,
}

impl Epoch {
    /// Start at the timeline origin (frame 0, cycle 0) at `cps`.
    pub fn start(cps: f64) -> Self {
        Epoch {
            frame: 0,
            cycle: Time::ZERO,
            cps,
        }
    }

    /// Output frames per cycle at this tempo.
    pub fn frames_per_cycle(&self, sample_rate: u32) -> f64 {
        sample_rate as f64 / self.cps
    }

    /// Absolute frame at which cycle-time `cycle` falls. Clamped at 0.
    pub fn frame_of(&self, cycle: Time, sample_rate: u32) -> u64 {
        let delta = (cycle - self.cycle).to_f64();
        let f = self.frame as f64 + delta * self.frames_per_cycle(sample_rate);
        if f <= 0.0 {
            0
        } else {
            f.round() as u64
        }
    }

    /// Like [`frame_of`](Self::frame_of) but **signed and unclamped**.
    ///
    /// `frame_of` clamps negative results to `0` (a real `start_frame` is never
    /// negative). That clamp is wrong for *filtering* onsets against a frame
    /// window: a look-ahead query is widened by a guard cycle, and the onsets of
    /// that earlier (possibly negative-frame) cycle would all collapse onto frame
    /// `0` and leak into the window. Filter with this signed frame instead, then
    /// build the event's real `start_frame` with `frame_of`.
    pub fn frame_of_signed(&self, cycle: Time, sample_rate: u32) -> i64 {
        let delta = (cycle - self.cycle).to_f64();
        (self.frame as f64 + delta * self.frames_per_cycle(sample_rate)).round() as i64
    }

    /// Cycle position (fractional) at absolute `frame`. Inverse of [`frame_of`](Self::frame_of).
    pub fn cycle_of(&self, frame: u64, sample_rate: u32) -> f64 {
        let delta_frames = frame as f64 - self.frame as f64;
        self.cycle.to_f64() + delta_frames / self.frames_per_cycle(sample_rate)
    }

    /// Re-anchor at a cycle boundary, keeping the frame continuous, so a new
    /// tempo (`new_cps`) takes effect from `at_cycle`/`at_frame` without a jump.
    pub fn reanchor(&self, at_cycle: Time, at_frame: u64, new_cps: f64) -> Epoch {
        Epoch {
            frame: at_frame,
            cycle: at_cycle,
            cps: new_cps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_mapping_round_trips() {
        let e = Epoch::start(1.0); // 1 cycle/s
        let sr = 48_000;
        assert_eq!(e.frames_per_cycle(sr), 48_000.0);
        assert_eq!(e.frame_of(Time::int(2), sr), 96_000);
        assert_eq!(e.frame_of(Time::new(1, 2), sr), 24_000);
        assert!((e.cycle_of(96_000, sr) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn reanchor_keeps_frame_continuous() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        // tempo doubles at cycle 4 (frame 192_000 under the old tempo)
        let boundary = Time::int(4);
        let at = e.frame_of(boundary, sr);
        let e2 = e.reanchor(boundary, at, 2.0);
        // cycle 5 is now one half-cycle-second later: 24_000 frames past the boundary
        assert_eq!(e2.frame_of(Time::int(5), sr), at + 24_000);
    }

    #[test]
    fn signed_frame_is_unclamped() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        assert_eq!(e.frame_of_signed(Time::int(1), sr), 48_000);
        assert_eq!(e.frame_of_signed(Time::ZERO, sr), 0);
        // A cycle before the epoch is a negative frame — NOT clamped to 0 the way
        // `frame_of` is. This is the distinction the scheduler's seam filter relies
        // on so a negative guard cycle can't collapse onto frame 0 and leak in.
        assert_eq!(e.frame_of_signed(Time::int(-1), sr), -48_000);
        assert_eq!(e.frame_of(Time::int(-1), sr), 0);
    }
}
