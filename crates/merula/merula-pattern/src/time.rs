//! Exact rational time.
//!
//! merula's clock is measured in **cycles** (see `semantics.md`): the position
//! inside a cycle is a fraction in `0..1`. Subdividing a cycle (`[a b c]`,
//! euclidean `(3,8)`, `fast(3)`) produces fractions like `1/3`, `1/6`, `1/12`
//! that `f64` cannot represent exactly — the rounding error accumulates and
//! haps stop landing on cycle/slot boundaries, which breaks both per-cycle
//! determinism and the offline render. So time is an **exact rational**, like
//! Tidal's `Rational`, hand-rolled to keep the crate dependency-free.
//!
//! Invariants kept by every constructor and operator:
//! - the denominator is always `> 0` (sign lives on the numerator),
//! - the fraction is always reduced (`gcd(num, den) == 1`),
//! - intermediate products use `i128` so deep nesting (`fast` inside `fast`
//!   inside an euclidean split) does not overflow before the result reduces.

use std::cmp::Ordering;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// An exact rational number of **cycles** (or a fraction of a cycle).
#[derive(Clone, Copy, Debug)]
pub struct Time {
    num: i64,
    den: i64,
}

/// Greatest common divisor (Euclid), always non-negative.
fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a as i64
}

/// Same, over `i128`, used while a product is still wide.
fn gcd128(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a as i128
}

/// Reduce a wide `num/den` and narrow it back to `i64` fields.
///
/// Panics on a zero denominator (a division-by-zero bug in a caller) or if the
/// reduced value genuinely does not fit in `i64` — acceptable for Fase 0: the
/// musical ranges in play never approach that magnitude, and a panic surfaces a
/// real bug rather than silently drifting.
fn reduce(mut num: i128, mut den: i128) -> Time {
    assert!(den != 0, "Time with zero denominator");
    if den < 0 {
        num = -num;
        den = -den;
    }
    let g = gcd128(num, den).max(1);
    num /= g;
    den /= g;
    Time {
        num: i64::try_from(num).expect("Time numerator overflow"),
        den: i64::try_from(den).expect("Time denominator overflow"),
    }
}

impl Time {
    /// `num / den`, reduced. `den` must be non-zero.
    pub fn new(num: i64, den: i64) -> Self {
        reduce(num as i128, den as i128)
    }

    /// A whole number of cycles.
    pub const fn int(n: i64) -> Self {
        Time { num: n, den: 1 }
    }

    pub const ZERO: Time = Time { num: 0, den: 1 };
    pub const ONE: Time = Time { num: 1, den: 1 };

    pub fn num(self) -> i64 {
        self.num
    }
    pub fn den(self) -> i64 {
        self.den
    }

    /// Largest integer `<= self` (the start cycle, Tidal's `sam` as an int).
    pub fn floor(self) -> i64 {
        self.num.div_euclid(self.den)
    }

    /// Start of the cycle containing `self` (`floor` as a `Time`).
    pub fn sam(self) -> Time {
        Time::int(self.floor())
    }

    /// Start of the next cycle.
    pub fn next_sam(self) -> Time {
        Time::int(self.floor() + 1)
    }

    /// Position inside the current cycle, in `0..1` (`self - sam`).
    pub fn cycle_pos(self) -> Time {
        self - self.sam()
    }

    /// Lossy view, for hashing/inspection only — never feed this back into the
    /// exact pipeline.
    pub fn to_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn min(self, other: Time) -> Time {
        if self <= other {
            self
        } else {
            other
        }
    }

    pub fn max(self, other: Time) -> Time {
        if self >= other {
            self
        } else {
            other
        }
    }
}

impl From<i64> for Time {
    fn from(n: i64) -> Self {
        Time::int(n)
    }
}

impl PartialEq for Time {
    fn eq(&self, other: &Self) -> bool {
        // Both operands are always reduced with a positive denominator, so a
        // field-wise compare is a true value compare.
        self.num == other.num && self.den == other.den
    }
}
impl Eq for Time {}

impl std::hash::Hash for Time {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.num.hash(state);
        self.den.hash(state);
    }
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> Ordering {
        // a/b vs c/d  ->  a*d vs c*b  (b, d > 0); widen to avoid overflow.
        let lhs = self.num as i128 * other.den as i128;
        let rhs = other.num as i128 * self.den as i128;
        lhs.cmp(&rhs)
    }
}

impl Add for Time {
    type Output = Time;
    fn add(self, rhs: Time) -> Time {
        let num = self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128;
        let den = self.den as i128 * rhs.den as i128;
        reduce(num, den)
    }
}

impl Sub for Time {
    type Output = Time;
    fn sub(self, rhs: Time) -> Time {
        let num = self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128;
        let den = self.den as i128 * rhs.den as i128;
        reduce(num, den)
    }
}

impl Mul for Time {
    type Output = Time;
    fn mul(self, rhs: Time) -> Time {
        reduce(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }
}

impl Div for Time {
    type Output = Time;
    fn div(self, rhs: Time) -> Time {
        assert!(rhs.num != 0, "Time division by zero");
        reduce(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }
}

impl Neg for Time {
    type Output = Time;
    fn neg(self) -> Time {
        Time {
            num: -self.num,
            den: self.den,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_and_normalizes_sign() {
        let t = Time::new(2, 4);
        assert_eq!((t.num(), t.den()), (1, 2));
        let n = Time::new(1, -2);
        assert_eq!((n.num(), n.den()), (-1, 2));
    }

    #[test]
    fn arithmetic_is_exact() {
        let third = Time::new(1, 3);
        let sum = third + third + third;
        assert_eq!(sum, Time::ONE); // no float drift
        assert_eq!(Time::new(1, 6) * Time::int(3), Time::new(1, 2));
        assert_eq!(Time::ONE / Time::int(3), third);
    }

    #[test]
    fn cycle_helpers() {
        let t = Time::new(7, 2); // 3.5
        assert_eq!(t.floor(), 3);
        assert_eq!(t.sam(), Time::int(3));
        assert_eq!(t.next_sam(), Time::int(4));
        assert_eq!(t.cycle_pos(), Time::new(1, 2));
        // negative position floors toward -inf
        let n = Time::new(-1, 2);
        assert_eq!(n.floor(), -1);
        assert_eq!(n.cycle_pos(), Time::new(1, 2));
    }

    #[test]
    fn ordering() {
        assert!(Time::new(1, 3) < Time::new(1, 2));
        assert!(Time::new(2, 3) > Time::new(1, 2));
        assert_eq!(Time::new(2, 4), Time::new(1, 2));
    }
}
