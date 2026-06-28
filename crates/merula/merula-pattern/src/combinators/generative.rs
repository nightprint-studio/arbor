//! Generative sources: `rand` and `choose`. They produce **values**, sampled
//! deterministically at the query's midpoint, so they read naturally as
//! patternised parameters (`.gain(rand(0.0, 1.0))`) or as spliced leaves.
//!
//! Both are **continuous signals**: every hap has `whole = None` (no onset),
//! only a value at each instant. Seeded per instant → identical every loop.

use crate::hap::Hap;
use crate::pattern::Pattern;
use crate::rng::{time_to_index, time_to_rand, SEED_CHOOSE, SEED_RAND};
use crate::span::TimeSpan;

/// A continuous pattern of floats uniformly in `[lo, hi)`. The range is
/// mandatory (no bare `0..1`), matching the language design.
pub fn rand(lo: f64, hi: f64) -> Pattern<f64> {
    Pattern::new(move |span: TimeSpan| {
        let r = time_to_rand(span.midpoint(), SEED_RAND);
        vec![Hap::new(None, span, lo + (hi - lo) * r)]
    })
}

/// A continuous pattern that picks one of `options` per query (seeded). Empty
/// options → silence.
pub fn choose<T: Clone + Send + Sync + 'static>(options: Vec<T>) -> Pattern<T> {
    Pattern::new(move |span: TimeSpan| {
        if options.is_empty() {
            return Vec::new();
        }
        let idx = time_to_index(span.midpoint(), SEED_CHOOSE, options.len());
        vec![Hap::new(None, span, options[idx].clone())]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Time;

    #[test]
    fn rand_in_range_and_continuous() {
        let p = rand(2.0, 4.0);
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert!(h.whole.is_none()); // continuous
        assert!((2.0..4.0).contains(&h.value));
    }

    #[test]
    fn rand_is_deterministic_per_instant() {
        let p = rand(0.0, 1.0);
        assert_eq!(p.value_at(Time::new(1, 3)), p.value_at(Time::new(1, 3)));
    }

    #[test]
    fn choose_picks_from_options() {
        let p = choose(vec!["a", "b", "c"]);
        for i in 0..20 {
            let v = p.value_at(Time::new(i, 7)).unwrap();
            assert!(["a", "b", "c"].contains(&v));
        }
    }

    #[test]
    fn choose_empty_is_silent() {
        let p: Pattern<&str> = choose(vec![]);
        assert!(p.query(TimeSpan::cycle(0)).is_empty());
    }
}
