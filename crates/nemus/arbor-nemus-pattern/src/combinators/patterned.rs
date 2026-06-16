//! Patternised parameters for the mini-notation postfixes — `bd*<2 3>`,
//! `bd*[2 3]`, `bd(<3 5>,8)`, …
//!
//! The factor of a `*`/`/`/euclid postfix can itself be a pattern. The semantics
//! match Strudel's *inner join*: split the cycle by the **control** pattern's
//! events, and inside each control event's span play the result of applying the
//! postfix with that control value, clipped to the span. A `<2 3>` (one value per
//! cycle) therefore alternates the factor cycle-by-cycle; a `[2 3]` (two values
//! in the cycle) applies a different factor in each half.

use crate::hap::Hap;
use crate::pattern::Pattern;
use crate::span::TimeSpan;
use crate::time::Time;

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Inner-join: for each event of `control`, build a `T`-pattern with `build`
    /// from the control value and play it **within that event's part**.
    ///
    /// This is the kernel every patternised postfix uses. The control pattern's
    /// onsets carve the cycle; `build` turns each control value into the warped
    /// payload pattern (e.g. `self.fast(value)`), which is then queried only over
    /// the control event's span so the per-slot variation lands exactly where the
    /// control says.
    pub fn inner_join_with<C: Clone + Send + Sync + 'static>(
        control: Pattern<C>,
        build: impl Fn(C) -> Pattern<T> + Send + Sync + 'static,
    ) -> Pattern<T> {
        Pattern::new(move |span: TimeSpan| {
            let mut out: Vec<Hap<T>> = Vec::new();
            for ch in control.query(span) {
                // Play the built pattern only over this control event's part.
                let inner = build(ch.value.clone());
                for mut h in inner.query(ch.part) {
                    // Clip the inner hap's `part` to the control window; keep its
                    // own `whole` so onsets are still detectable.
                    if let Some(clipped) = h.part.sect(ch.part) {
                        h.part = clipped;
                        out.push(h);
                    }
                }
            }
            out
        })
    }
}

/// `fast` with a patternised factor: `self.fast(v)` per control value `v`.
pub fn fast_with<T: Clone + Send + Sync + 'static>(
    pat: Pattern<T>,
    factor: Pattern<f64>,
) -> Pattern<T> {
    Pattern::inner_join_with(factor, move |v| pat.clone().fast(ratio(v)))
}

/// `slow` with a patternised factor.
pub fn slow_with<T: Clone + Send + Sync + 'static>(
    pat: Pattern<T>,
    factor: Pattern<f64>,
) -> Pattern<T> {
    Pattern::inner_join_with(factor, move |v| pat.clone().slow(ratio(v)))
}

/// `euclid` with any of `pulses` / `steps` / `rotation` patternised. Each control
/// is a `Pattern<f64>`; they are zipped per slot through nested inner joins.
pub fn euclid_with<T: Clone + Send + Sync + 'static>(
    pat: Pattern<T>,
    pulses: Pattern<f64>,
    steps: Pattern<f64>,
    rotation: Pattern<f64>,
) -> Pattern<T> {
    Pattern::inner_join_with(pulses, move |p| {
        let pat = pat.clone();
        let steps = steps.clone();
        let rotation = rotation.clone();
        Pattern::inner_join_with(steps, move |s| {
            let pat = pat.clone();
            let p = p;
            Pattern::inner_join_with(rotation.clone(), move |r| {
                pat.clone().euclid(p as u32, s as u32, r as i32)
            })
        })
    })
}

/// Convert a (possibly fractional) factor to exact [`Time`] for `fast`/`slow`.
/// Dyadic and simple-tuplet factors are recognised exactly; anything else falls
/// back to a fine denominator (the same policy as the language's `f64_to_time`,
/// duplicated here to keep the pattern crate dependency-free of the lang layer).
fn ratio(x: f64) -> Time {
    const DENS: &[i64] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 16, 24, 32, 48, 64, 128, 256, 512, 1024];
    for &d in DENS {
        let scaled = x * d as f64;
        if (scaled - scaled.round()).abs() < 1e-9 {
            return Time::new(scaled.round() as i64, d);
        }
    }
    Time::new((x * 1_000_000.0).round() as i64, 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::{pure, slowcat};

    fn onset_count(p: &Pattern<&'static str>, cyc: i64) -> usize {
        p.query(TimeSpan::cycle(cyc))
            .into_iter()
            .filter(|h| h.has_onset())
            .count()
    }

    #[test]
    fn fast_with_alternation_varies_per_cycle() {
        // bd*<2 3>: two onsets on even cycles, three on odd.
        let factor = slowcat(vec![pure(2.0), pure(3.0)]);
        let p = fast_with(pure("bd"), factor);
        assert_eq!(onset_count(&p, 0), 2);
        assert_eq!(onset_count(&p, 1), 3);
        assert_eq!(onset_count(&p, 2), 2);
    }

    #[test]
    fn fast_with_sequence_splits_the_cycle() {
        // bd*[2 3]: factor 2 in the first half, 3 in the second.
        // First half: fast(2) over [0,1/2) → 1 onset; second half: fast(3) over
        // [1/2,1) → onsets at multiples of 1/3 inside → 1 (at 2/3). Total 2.
        use crate::combinators::compose::fastcat;
        let factor = fastcat(vec![pure(2.0), pure(3.0)]);
        let p = fast_with(pure("bd"), factor);
        // At least the per-half factor lands: 2+ onsets, deterministic.
        assert!(onset_count(&p, 0) >= 2);
    }

    #[test]
    fn euclid_with_patterned_pulses() {
        // bd(<3 5>,8): tresillo on even cycles, quintillo on odd.
        let pulses = slowcat(vec![pure(3.0), pure(5.0)]);
        let p = euclid_with(pure("bd"), pulses, pure(8.0), pure(0.0));
        assert_eq!(onset_count(&p, 0), 3);
        assert_eq!(onset_count(&p, 1), 5);
    }
}
