//! Time & structure transforms: `fast`, `slow`, `rev`, `every`, `off` (plus the
//! `late`/`early` shift primitives they build on).
//!
//! These are methods on `Pattern<T>` — value-agnostic, so they work for any
//! payload. The transform-value / partial-application duality (`fast(2)` as a
//! standalone value) is a language-layer concern (Fase 1); here a "transform
//! value" passed to `every`/`off` is simply a Rust closure.

use crate::combinators::compose::{silence, stack};
use crate::hap::Hap;
use crate::pattern::Pattern;
use crate::span::TimeSpan;
use crate::time::Time;

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Compress time by factor `n`: `n`× more repetitions per cycle. `fast(2)`
    /// doubles the speed, `fast(0.5)` halves it. `n < 0` reverses; `n == 0` is
    /// silence.
    pub fn fast(self, n: Time) -> Pattern<T> {
        if n == Time::ZERO {
            return silence();
        }
        if n < Time::ZERO {
            return self.fast(-n).rev();
        }
        self.with_query_time(move |t| t * n).with_hap_time(move |t| t / n)
    }

    /// Dilate time: `slow(n) == fast(1/n)`. The pattern takes `n` cycles.
    pub fn slow(self, n: Time) -> Pattern<T> {
        if n == Time::ZERO {
            return silence();
        }
        self.fast(Time::ONE / n)
    }

    /// Shift every event **later** by `t` cycles (Tidal's `rotR`).
    pub fn late(self, t: Time) -> Pattern<T> {
        self.with_query_time(move |x| x - t)
            .with_hap_time(move |x| x + t)
    }

    /// Shift every event **earlier** by `t` cycles.
    pub fn early(self, t: Time) -> Pattern<T> {
        self.late(-t)
    }

    /// Reverse the order of events within each cycle.
    pub fn rev(self) -> Pattern<T> {
        Pattern::new(move |span: TimeSpan| {
            // Reflect around the cycle the (single-cycle) query sits in.
            let cyc = span.begin.sam();
            let next = cyc.next_sam();
            let reflect = move |t: Time| cyc + next - t;
            let reflect_span =
                move |s: TimeSpan| TimeSpan::new(reflect(s.end), reflect(s.begin));

            let queried = self.query(reflect_span(span));
            queried
                .into_iter()
                .map(|h| Hap {
                    whole: h.whole.map(reflect_span),
                    part: reflect_span(h.part),
                    value: h.value,
                    span: h.span,
                })
                .collect()
        })
        .split_queries()
    }

    /// Apply `f` on cycles `0, n, 2n, …`, leaving the others unchanged.
    /// `f` is a transform value (a closure here in Fase 0).
    pub fn every(self, n: i64, f: impl FnOnce(Pattern<T>) -> Pattern<T>) -> Pattern<T> {
        if n <= 0 {
            return self;
        }
        let transformed = f(self.clone());
        let original = self;
        Pattern::new(move |span: TimeSpan| {
            let cyc = span.begin.floor();
            if cyc.rem_euclid(n) == 0 {
                transformed.query(span)
            } else {
                original.query(span)
            }
        })
        .split_queries()
    }

    /// Overlay a copy shifted **later** by `t`, with `f` applied to that copy
    /// (echoes / layers). `t` in cycles.
    pub fn off(self, t: Time, f: impl FnOnce(Pattern<T>) -> Pattern<T>) -> Pattern<T> {
        let copy = f(self.clone().late(t));
        stack(vec![self, copy])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::{fastcat, pure};

    /// Query results are not guaranteed to be in time order (Tidal-style —
    /// consumers sort). Sort by onset before reading off the sequence.
    fn vals_in_time<T: Clone>(haps: &[Hap<T>]) -> Vec<T> {
        let mut sorted: Vec<&Hap<T>> = haps.iter().collect();
        sorted.sort_by_key(|h| h.part.begin);
        sorted.into_iter().map(|h| h.value.clone()).collect()
    }

    #[test]
    fn fast_doubles_events_per_cycle() {
        let p = pure("x").fast(Time::int(2));
        let haps = p.query(TimeSpan::cycle(0));
        assert_eq!(haps.len(), 2);
        assert_eq!(haps[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::new(1, 2)));
        assert_eq!(haps[1].whole.unwrap(), TimeSpan::new(Time::new(1, 2), Time::ONE));
    }

    #[test]
    fn slow_stretches_over_cycles() {
        // pure over 2 cycles, slowed by 2 → one event spanning [0,2)
        let p = pure("x").slow(Time::int(2));
        let haps = p.query(TimeSpan::new(Time::ZERO, Time::int(2)));
        // onset only in cycle 0
        let onsets: Vec<_> = haps.iter().filter(|h| h.has_onset()).collect();
        assert_eq!(onsets.len(), 1);
        assert_eq!(onsets[0].whole.unwrap(), TimeSpan::new(Time::ZERO, Time::int(2)));
    }

    #[test]
    fn rev_reverses_within_cycle() {
        let p = fastcat(vec![pure("a"), pure("b"), pure("c")]).rev();
        let haps = p.query(TimeSpan::cycle(0));
        assert_eq!(vals_in_time(&haps), vec!["c", "b", "a"]);
    }

    #[test]
    fn late_shifts_forward() {
        let p = pure("x").late(Time::new(1, 4));
        let haps = p.query(TimeSpan::cycle(0));
        let onset = haps.iter().find(|h| h.has_onset()).unwrap();
        assert_eq!(onset.whole.unwrap().begin, Time::new(1, 4));
    }

    #[test]
    fn every_applies_on_matching_cycles() {
        let p = fastcat(vec![pure("a"), pure("b")]).every(2, |q| q.rev());
        // cycle 0: reversed → b, a
        assert_eq!(vals_in_time(&p.query(TimeSpan::cycle(0))), vec!["b", "a"]);
        // cycle 1: untouched → a, b
        assert_eq!(vals_in_time(&p.query(TimeSpan::cycle(1))), vec!["a", "b"]);
    }

    #[test]
    fn off_overlays_shifted_copy() {
        let p = pure("x").off(Time::new(1, 2), |q| q);
        let haps = p.query(TimeSpan::cycle(0));
        let onsets: Vec<_> = haps.iter().filter(|h| h.has_onset()).map(|h| h.whole.unwrap().begin).collect();
        assert!(onsets.contains(&Time::ZERO));
        assert!(onsets.contains(&Time::new(1, 2)));
    }
}
