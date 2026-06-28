//! Deterministic, time-seeded randomness.
//!
//! merula's "random" must be **reproducible**: the same cycle always yields the
//! same choice (`semantics.md`), so a loop sounds identical each pass and a
//! re-eval after an edit never disturbs cycles already fixed. There is no
//! mutable RNG state — randomness is a *pure function of the time coordinate*
//! (plus a per-call seed to decorrelate independent generators).
//!
//! Hand-rolled (no `rand` crate): a SplitMix64 finalizer over the exact
//! rational's reduced `num`/`den`. Because every [`Time`] is canonically
//! reduced, a given instant hashes to a stable key.

use crate::time::Time;

// Distinct per-generator seeds so independent random sources (degrade vs.
// sometimes vs. rand vs. choose) never correlate at the same instant. The
// values are arbitrary but fixed — changing one only reshuffles that one
// generator's choices.
pub const SEED_DEGRADE: u64 = 0x10ce_5eed_de92_ade5;
pub const SEED_SOMETIMES: u64 = 0x5044_e715_0e71_e500;
pub const SEED_RAND: u64 = 0x2a2a_2a2a_2a2a_2a2a;
pub const SEED_CHOOSE: u64 = 0xc400_5e00_c400_5e00;
// `humanize` jitters timing and gain off two independent streams so the micro
// shift of an onset never correlates with its loudness wobble.
pub const SEED_HUMANIZE_TIME: u64 = 0x4855_4d41_4e7a_5449;
pub const SEED_HUMANIZE_GAIN: u64 = 0x4855_4d41_4e7a_4741;

/// SplitMix64 finalizer — a strong integer bit-mixer.
fn mix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A uniform `f64` in `[0, 1)` derived deterministically from `time` and
/// `seed`. Different `seed`s give independent streams at the same instant.
pub fn time_to_rand(time: Time, seed: u64) -> f64 {
    let n = mix64(time.num() as u64);
    let d = mix64((time.den() as u64).rotate_left(32) ^ 0x9e37_79b9_7f4a_7c15);
    let s = mix64(seed ^ 0xa076_1d64_78bd_642f);
    let bits = mix64(n ^ d.rotate_left(17) ^ s);
    // Top 53 bits → [0, 1), the standard f64-from-u64 construction.
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

/// Pick an index in `0..len` deterministically. `len` must be `> 0`.
pub fn time_to_index(time: Time, seed: u64, len: usize) -> usize {
    debug_assert!(len > 0);
    let r = time_to_rand(time, seed);
    ((r * len as f64) as usize).min(len - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_per_instant() {
        let t = Time::new(3, 8);
        assert_eq!(time_to_rand(t, 0), time_to_rand(t, 0));
    }

    #[test]
    fn seed_decorrelates() {
        let t = Time::new(3, 8);
        assert_ne!(time_to_rand(t, 0), time_to_rand(t, 1));
    }

    #[test]
    fn in_unit_range() {
        for i in 0..200 {
            let r = time_to_rand(Time::new(i, 7), 42);
            assert!((0.0..1.0).contains(&r), "{r} out of range");
        }
    }

    #[test]
    fn index_within_bounds() {
        for i in 0..200 {
            let idx = time_to_index(Time::new(i, 3), 1, 5);
            assert!(idx < 5);
        }
    }

    #[test]
    fn roughly_uniform() {
        // Sanity: mean over many points should sit near 0.5.
        let n = 5000;
        let mean: f64 = (0..n).map(|i| time_to_rand(Time::new(i, 1), 7)).sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 0.05, "mean was {mean}");
    }
}
