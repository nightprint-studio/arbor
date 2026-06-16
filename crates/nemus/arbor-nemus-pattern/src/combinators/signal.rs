//! Continuous unipolar signal sources: `sine`, `saw`, `isaw`, `tri`, `square`,
//! plus `.range(lo, hi)` to rescale a `0..1` signal into `[lo, hi]`.
//!
//! A signal is a **continuous** pattern (`whole = None`): it has a value at every
//! instant, sampled at the query midpoint, and no onset. Each completes **one
//! period per cycle** by reading the cycle-relative position
//! ([`Time::cycle_pos`]), so the existing `fast`/`slow` (which warp query time)
//! make them run faster/slower for free — `sine.fast(2)` is two periods a cycle.
//!
//! They are `Pattern<f64>`, the numeric-signal arm of the value model, so they
//! feed patternised controls (`.lpf(sine.range(200.0, 2000.0))`).

use crate::hap::Hap;
use crate::pattern::Pattern;
use crate::span::TimeSpan;
use crate::time::Time;

/// Build a continuous `Pattern<f64>` from a phase→value waveform. `phase` is the
/// cycle-relative position in `0..1`; `wave` returns the unipolar `0..1` sample.
fn signal(wave: impl Fn(f64) -> f64 + Send + Sync + 'static) -> Pattern<f64> {
    Pattern::new(move |span: TimeSpan| {
        let phase = span.midpoint().cycle_pos().to_f64();
        vec![Hap::new(None, span, wave(phase))]
    })
}

/// A sine that completes one period per cycle, mapped to `0..1`
/// (`(sin(2πp) + 1) / 2`). Starts at `0.5`, peaks at `1` a quarter in.
pub fn sine() -> Pattern<f64> {
    signal(|p| (f64::sin(std::f64::consts::TAU * p) + 1.0) / 2.0)
}

/// A rising sawtooth: ramps linearly `0 → 1` across the cycle, then resets.
pub fn saw() -> Pattern<f64> {
    signal(|p| p)
}

/// A falling sawtooth (inverse of [`saw`]): ramps `1 → 0` across the cycle.
pub fn isaw() -> Pattern<f64> {
    signal(|p| 1.0 - p)
}

/// A triangle: rises `0 → 1` over the first half, falls `1 → 0` over the second.
pub fn tri() -> Pattern<f64> {
    signal(|p| if p < 0.5 { p * 2.0 } else { 2.0 - p * 2.0 })
}

/// A square: `1` for the first half of the cycle, `0` for the second.
pub fn square() -> Pattern<f64> {
    signal(|p| if p < 0.5 { 1.0 } else { 0.0 })
}

impl Pattern<f64> {
    /// Rescale a unipolar `0..1` signal into `[lo, hi]` (`lo + v·(hi - lo)`).
    /// Values outside `0..1` scale linearly too (no clamping), so a bipolar
    /// source would map proportionally.
    pub fn range(self, lo: f64, hi: f64) -> Pattern<f64> {
        self.fmap(move |v| lo + v * (hi - lo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(p: &Pattern<f64>, pos: Time) -> f64 {
        p.value_at(pos).unwrap()
    }

    #[test]
    fn signals_are_continuous() {
        for p in [sine(), saw(), isaw(), tri(), square()] {
            let h = &p.query(TimeSpan::cycle(0))[0];
            assert!(h.whole.is_none(), "signal hap must be continuous");
        }
    }

    #[test]
    fn saw_ramps_zero_to_one() {
        let p = saw();
        assert!((at(&p, Time::ZERO) - 0.0).abs() < 1e-9);
        assert!((at(&p, Time::new(1, 2)) - 0.5).abs() < 1e-9);
        // second cycle restarts the ramp.
        assert!((at(&p, Time::new(5, 4)) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn isaw_is_the_inverse_of_saw() {
        let (s, i) = (saw(), isaw());
        for k in 0..8 {
            let t = Time::new(k, 8);
            assert!((at(&s, t) + at(&i, t) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn tri_peaks_at_the_midpoint() {
        let p = tri();
        assert!((at(&p, Time::new(1, 2)) - 1.0).abs() < 1e-9);
        assert!(at(&p, Time::new(1, 4)) < at(&p, Time::new(1, 2)));
    }

    #[test]
    fn square_is_high_then_low() {
        let p = square();
        assert_eq!(at(&p, Time::new(1, 4)), 1.0);
        assert_eq!(at(&p, Time::new(3, 4)), 0.0);
    }

    #[test]
    fn sine_stays_unipolar_and_centres() {
        let p = sine();
        for k in 0..16 {
            let v = at(&p, Time::new(k, 16));
            assert!((0.0..=1.0).contains(&v), "sine out of 0..1: {v}");
        }
        assert!((at(&p, Time::ZERO) - 0.5).abs() < 1e-9); // sin(0) → 0.5
    }

    #[test]
    fn range_rescales_unit_signal() {
        let p = saw().range(200.0, 2000.0);
        assert!((at(&p, Time::ZERO) - 200.0).abs() < 1e-6);
        assert!((at(&p, Time::new(1, 2)) - 1100.0).abs() < 1e-6);
    }

    #[test]
    fn fast_runs_more_periods_per_cycle() {
        // saw.fast(2): two ramps a cycle → value at 1/4 equals plain saw at 1/2.
        let fast_saw = saw().fast(Time::int(2));
        assert!((at(&fast_saw, Time::new(1, 4)) - 0.5).abs() < 1e-9);
    }
}
