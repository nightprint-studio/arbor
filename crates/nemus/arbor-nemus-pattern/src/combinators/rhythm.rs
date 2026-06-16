//! Rhythm & probability transforms: `degrade`, `sometimes` (and their explicit
//! `*_by` variants). `jux` lives in [`crate::combinators::voice`] because it
//! sets `pan`, which is a `ControlMap` concern.
//!
//! All randomness is **seeded by the event's onset time** (see [`crate::rng`]),
//! so the same cycle always makes the same choices and a re-eval never disturbs
//! cycles already played.

use crate::combinators::compose::{fastcat, silence, stack};
use crate::pattern::Pattern;
use crate::rng::{time_to_rand, SEED_DEGRADE, SEED_SOMETIMES};

/// Bjorklund's algorithm: spread `pulses` onsets as evenly as possible across
/// `steps` slots, returning the on/off mask (the classic Euclidean rhythm).
/// `pulses` is clamped to `steps`.
fn bjorklund(pulses: u32, steps: u32) -> Vec<bool> {
    let steps = steps as usize;
    let pulses = (pulses as usize).min(steps);
    if steps == 0 {
        return Vec::new();
    }
    if pulses == 0 {
        return vec![false; steps];
    }
    // Repeatedly pair the longer run of sequences onto the shorter one until at
    // most one remainder sequence is left — the standard pairing construction.
    let mut a: Vec<Vec<bool>> = vec![vec![true]; pulses];
    let mut b: Vec<Vec<bool>> = vec![vec![false]; steps - pulses];
    while b.len() > 1 {
        let n = a.len().min(b.len());
        let mut paired = Vec::with_capacity(n);
        for i in 0..n {
            let mut seq = a[i].clone();
            seq.extend_from_slice(&b[i]);
            paired.push(seq);
        }
        let remainder = if a.len() > b.len() {
            a[n..].to_vec()
        } else {
            b[n..].to_vec()
        };
        a = paired;
        b = remainder;
    }
    a.into_iter().chain(b).flatten().collect()
}

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

    /// Euclidean rhythm: play this pattern on the `pulses`-of-`steps` onsets
    /// (Bjorklund), resting on the others; `rotation` rotates the mask left.
    /// Mini-notation `bd(3,8)` / `bd(3,8,2)`. `steps == 0` → silence.
    pub fn euclid(self, pulses: u32, steps: u32, rotation: i32) -> Pattern<T> {
        if steps == 0 {
            return silence();
        }
        let mut mask = bjorklund(pulses, steps);
        mask.rotate_left(rotation.rem_euclid(steps as i32) as usize);
        let slots = mask
            .into_iter()
            .map(|on| if on { self.clone() } else { silence() })
            .collect();
        fastcat(slots)
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

    #[test]
    fn bjorklund_classic_patterns() {
        let t = true;
        let f = false;
        assert_eq!(bjorklund(3, 8), vec![t, f, f, t, f, f, t, f]); // tresillo
        assert_eq!(bjorklund(0, 4), vec![f, f, f, f]);
        assert_eq!(bjorklund(4, 4), vec![t, t, t, t]);
        assert_eq!(bjorklund(5, 3).len(), 3); // pulses clamped to steps
    }

    #[test]
    fn euclid_places_onsets_evenly() {
        let p = pure("x").euclid(3, 8, 0);
        let mut starts: Vec<_> = p
            .query(TimeSpan::cycle(0))
            .into_iter()
            .filter(|h| h.has_onset())
            .map(|h| h.whole.unwrap().begin)
            .collect();
        starts.sort();
        assert_eq!(starts, vec![Time::ZERO, Time::new(3, 8), Time::new(6, 8)]);
    }

    #[test]
    fn euclid_rotation_shifts_the_mask() {
        // rotate the tresillo left by one step → first onset moves off the downbeat.
        let p = pure("x").euclid(3, 8, 1);
        let has_downbeat = p
            .query(TimeSpan::cycle(0))
            .into_iter()
            .any(|h| h.has_onset() && h.whole.unwrap().begin == Time::ZERO);
        assert!(!has_downbeat);
    }
}
