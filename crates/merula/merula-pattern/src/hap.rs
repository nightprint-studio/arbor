//! A single event produced by querying a pattern.

use crate::span::{SourceSpan, TimeSpan};
use crate::time::Time;

/// One thing that happens: a value, *when* it happens, and *where* in the
/// source it came from.
///
/// Shape follows Tidal (`whole` / `part` / `value`) plus merula's `span`:
///
/// - `whole` — the event's full extent. `None` for **continuous** signals
///   (`rand`, `choose`) that have no onset, only a value at every instant.
/// - `part` — the fragment actually covered by the query, always clipped to the
///   query window. For a discrete event queried whole, `part == whole`.
/// - `value` — the payload (generic; the real merula value is
///   [`crate::control::ControlMap`]).
/// - `span` — byte range in the `.merula` source for live highlight; `None` for
///   hand-built / generated patterns.
///
/// A hap **has an onset** in this query when `part.begin == whole.begin`: the
/// query caught the moment the event starts (vs. a tail fragment).
#[derive(Clone, Debug, PartialEq)]
pub struct Hap<T> {
    pub whole: Option<TimeSpan>,
    pub part: TimeSpan,
    pub value: T,
    pub span: Option<SourceSpan>,
}

impl<T> Hap<T> {
    pub fn new(whole: Option<TimeSpan>, part: TimeSpan, value: T) -> Self {
        Hap {
            whole,
            part,
            value,
            span: None,
        }
    }

    /// Builder-style attach of a source span.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// `true` when the query captured the event's start.
    pub fn has_onset(&self) -> bool {
        matches!(self.whole, Some(w) if w.begin == self.part.begin)
    }

    /// The onset time, falling back to the part start for continuous signals.
    pub fn onset(&self) -> Time {
        self.whole.map_or(self.part.begin, |w| w.begin)
    }

    /// Map the value, preserving timing and source span.
    pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> Hap<U> {
        Hap {
            whole: self.whole,
            part: self.part,
            value: f(self.value),
            span: self.span,
        }
    }

    /// Map every time coordinate (`whole` and `part`) through `f`.
    pub fn map_time(self, f: impl Fn(Time) -> Time) -> Hap<T> {
        Hap {
            whole: self.whole.map(|w| w.with_time(&f)),
            part: self.part.with_time(&f),
            value: self.value,
            span: self.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(b: i64, e: i64) -> TimeSpan {
        TimeSpan::new(Time::int(b), Time::int(e))
    }

    #[test]
    fn onset_detection() {
        let whole = span(0, 1);
        let full = Hap::new(Some(whole), whole, "bd");
        assert!(full.has_onset());

        let tail = Hap::new(Some(whole), TimeSpan::new(Time::new(1, 2), Time::int(1)), "bd");
        assert!(!tail.has_onset());

        let continuous = Hap::new(None, whole, 0.5);
        assert!(!continuous.has_onset());
        assert_eq!(continuous.onset(), Time::ZERO);
    }
}
