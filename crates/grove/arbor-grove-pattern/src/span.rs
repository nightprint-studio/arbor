//! Time spans (query/result windows) and source spans (editor highlight).

use crate::time::Time;

/// A half-open interval of cycle time `[begin, end)`.
///
/// Used both as the **query window** handed to a pattern and as a hap's `whole`
/// / `part` (see [`crate::hap::Hap`]). A zero-width span (`begin == end`) is a
/// valid instant query — continuous signals (`rand`) answer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeSpan {
    pub begin: Time,
    pub end: Time,
}

impl TimeSpan {
    pub fn new(begin: Time, end: Time) -> Self {
        TimeSpan { begin, end }
    }

    /// The single cycle `[n, n+1)`.
    pub fn cycle(n: i64) -> Self {
        TimeSpan::new(Time::int(n), Time::int(n + 1))
    }

    pub fn duration(self) -> Time {
        self.end - self.begin
    }

    pub fn is_empty(self) -> bool {
        self.begin >= self.end
    }

    /// Midpoint — the sample point for continuous signals.
    pub fn midpoint(self) -> Time {
        (self.begin + self.end) / Time::int(2)
    }

    /// Map both endpoints through `f`.
    pub fn with_time(self, f: impl Fn(Time) -> Time) -> Self {
        TimeSpan::new(f(self.begin), f(self.end))
    }

    /// Intersection, or `None` if they don't overlap. A shared endpoint only
    /// counts when one side is a zero-width instant sitting on it (so an
    /// instant query at `begin` still hits a `[begin, end)` event).
    pub fn sect(self, other: TimeSpan) -> Option<TimeSpan> {
        let begin = self.begin.max(other.begin);
        let end = self.end.min(other.end);
        if begin < end {
            Some(TimeSpan::new(begin, end))
        } else if begin == end && (self.is_zero_width() || other.is_zero_width()) {
            Some(TimeSpan::new(begin, end))
        } else {
            None
        }
    }

    fn is_zero_width(self) -> bool {
        self.begin == self.end
    }

    /// Split this span at every integer cycle boundary it crosses.
    ///
    /// Many combinators are defined "within one cycle"; querying them is only
    /// correct if the incoming span never straddles a boundary. This is the
    /// backbone of [`crate::pattern::Pattern::split_queries`].
    pub fn split_cycles(self) -> Vec<TimeSpan> {
        if self.is_empty() {
            // Preserve a zero-width instant query as-is.
            return if self.begin == self.end {
                vec![self]
            } else {
                vec![]
            };
        }
        let mut out = Vec::new();
        let mut cursor = self.begin;
        while cursor < self.end {
            let next = cursor.next_sam().min(self.end);
            out.push(TimeSpan::new(cursor, next));
            cursor = next;
        }
        out
    }
}

/// Byte offsets into the original `.grove` source, carried by haps from day one
/// so the live editor can highlight exactly the characters that are sounding.
/// Optional on a hap: patterns built by hand or generated have no source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub fn new(start: u32, end: u32) -> Self {
        SourceSpan { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(n: i64, d: i64) -> Time {
        Time::new(n, d)
    }

    #[test]
    fn sect_basic() {
        let a = TimeSpan::new(t(0, 1), t(1, 1));
        let b = TimeSpan::new(t(1, 2), t(3, 2));
        assert_eq!(a.sect(b), Some(TimeSpan::new(t(1, 2), t(1, 1))));
        let c = TimeSpan::new(t(2, 1), t(3, 1));
        assert_eq!(a.sect(c), None);
    }

    #[test]
    fn sect_instant_on_boundary_hits() {
        let event = TimeSpan::new(t(0, 1), t(1, 1));
        let instant = TimeSpan::new(t(0, 1), t(0, 1));
        assert_eq!(instant.sect(event), Some(instant));
    }

    #[test]
    fn split_cycles_breaks_on_boundaries() {
        let span = TimeSpan::new(t(1, 2), t(5, 2)); // 0.5 .. 2.5
        let parts = span.split_cycles();
        assert_eq!(
            parts,
            vec![
                TimeSpan::new(t(1, 2), t(1, 1)),
                TimeSpan::new(t(1, 1), t(2, 1)),
                TimeSpan::new(t(2, 1), t(5, 2)),
            ]
        );
    }

    #[test]
    fn split_cycles_single_cycle_untouched() {
        let span = TimeSpan::cycle(3);
        assert_eq!(span.split_cycles(), vec![span]);
    }
}
