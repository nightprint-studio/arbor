//! Rhythm & probability transforms: `degrade`, `sometimes` (and their explicit
//! `*_by` variants). `jux` lives in [`crate::combinators::voice`] because it
//! sets `pan`, which is a `ControlMap` concern.
//!
//! All randomness is **seeded by the event's onset time** (see [`crate::rng`]),
//! so the same cycle always makes the same choices and a re-eval never disturbs
//! cycles already played.

use crate::combinators::compose::stack;
use crate::pattern::Pattern;
use crate::rng::{time_to_rand, SEED_DEGRADE, SEED_SOMETIMES};

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Keep each event with probability `1 - prob` (drop a `prob` fraction),
    /// deterministically per onset. `seed` selects the random stream.
    pub fn degrade_by_seeded(self, prob: f64, seed: u64) -> Pattern<T> {
        Pattern::new(move |span| {
            self.query(span)
                .into_iter()
                .filter(|h| time_to_rand(h.onset(), seed) >= prob)
                .collect()
        })
    }

    /// Complement of [`degrade_by_seeded`](Self::degrade_by_seeded): keep the
    /// events the other would drop. Used to build `sometimes`.
    fn undegrade_by_seeded(self, prob: f64, seed: u64) -> Pattern<T> {
        Pattern::new(move |span| {
            self.query(span)
                .into_iter()
                .filter(|h| time_to_rand(h.onset(), seed) < prob)
                .collect()
        })
    }

    /// Drop a `prob` fraction of events (deterministic).
    pub fn degrade_by(self, prob: f64) -> Pattern<T> {
        self.degrade_by_seeded(prob, SEED_DEGRADE)
    }

    /// Drop ~50% of events (deterministic).
    pub fn degrade(self) -> Pattern<T> {
        self.degrade_by(0.5)
    }

    /// Apply `f` to a `prob` fraction of events, leaving the rest unchanged
    /// (deterministic per onset). `f` is a transform value (closure in Fase 0).
    pub fn sometimes_by(self, prob: f64, f: impl FnOnce(Pattern<T>) -> Pattern<T>) -> Pattern<T> {
        let untouched = self.clone().degrade_by_seeded(prob, SEED_SOMETIMES);
        let affected = f(self.undegrade_by_seeded(prob, SEED_SOMETIMES));
        stack(vec![untouched, affected])
    }

    /// Apply `f` to ~50% of events.
    pub fn sometimes(self, f: impl FnOnce(Pattern<T>) -> Pattern<T>) -> Pattern<T> {
        self.sometimes_by(0.5, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::pure;
    use crate::span::TimeSpan;
    use crate::time::Time;

    /// Count onsets of a dense pattern over many cycles.
    fn onset_count(p: &Pattern<&'static str>, cycles: i64) -> usize {
        (0..cycles)
            .flat_map(|c| p.query(TimeSpan::cycle(c)))
            .filter(|h| h.has_onset())
            .count()
    }

    #[test]
    fn degrade_is_deterministic() {
        let p = pure("x").fast(Time::int(8)).degrade();
        let a = p.query(TimeSpan::cycle(3));
        let b = p.query(TimeSpan::cycle(3));
        assert_eq!(a.len(), b.len());
        // same onsets both times
        let oa: Vec<_> = a.iter().map(|h| h.onset()).collect();
        let ob: Vec<_> = b.iter().map(|h| h.onset()).collect();
        assert_eq!(oa, ob);
    }

    #[test]
    fn degrade_drops_roughly_half() {
        let full = pure("x").fast(Time::int(16));
        let degraded = full.clone().degrade();
        let kept = onset_count(&degraded, 32);
        let total = onset_count(&full, 32); // 16 * 32 = 512
        let ratio = kept as f64 / total as f64;
        assert!((0.4..0.6).contains(&ratio), "kept ratio {ratio}");
    }

    #[test]
    fn sometimes_keeps_all_events_but_transforms_some() {
        // identity-vs-tagged isn't observable on &str without a transform that
        // changes value; instead assert the event count is preserved (untouched
        // + affected partition the originals).
        let p = pure("x").fast(Time::int(8)).sometimes(|q| q);
        assert_eq!(onset_count(&p, 8), 8 * 8);
    }
}
