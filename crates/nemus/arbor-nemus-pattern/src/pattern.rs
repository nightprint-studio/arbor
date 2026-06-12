//! The pattern itself: a pure function from a query window to events.
//!
//! `Pattern = (TimeSpan) -> [Hap]`. There is **no hidden state** — everything a
//! pattern produces is a function of the query time, which is what makes
//! live-edit, determinism and offline render simple (`semantics.md`). The time
//! line is **absolute**: a pattern is queried at the true cycle `N`, never
//! auto-wrapped to cycle 0; looping is a transform you opt into, not a baked-in
//! behaviour.
//!
//! This module holds the type plus the small set of **internal query helpers**
//! (`split_queries`, `with_query_time`, `with_hap_time`, `fmap`, `filter_haps`,
//! `value_at`) that every combinator in [`crate::combinators`] is built from —
//! the same factoring Tidal uses.

use std::sync::Arc;

use crate::hap::Hap;
use crate::span::{SourceSpan, TimeSpan};
use crate::time::Time;

/// The boxed query function. `Send + Sync` so the engine can drive a pattern
/// from its worker thread; `Arc` so transforms can cheaply share/wrap a source
/// pattern without recomputing anything.
type Query<T> = Arc<dyn Fn(TimeSpan) -> Vec<Hap<T>> + Send + Sync>;

/// A pattern of values of type `T`.
///
/// Generic over the payload so structural combinators (`stack`, `cat`, `fast`,
/// `rev`, …) stay completely value-agnostic and can be unit-tested with trivial
/// values (`Pattern<i32>`, `Pattern<&str>`). The real nemus pattern is
/// `Pattern<ControlMap>`; voice/mix transforms live in an `impl` specialised to
/// that payload.
#[derive(Clone)]
pub struct Pattern<T> {
    query: Query<T>,
}

impl<T> std::fmt::Debug for Pattern<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The query is an opaque closure; nothing useful to print.
        f.debug_struct("Pattern").finish_non_exhaustive()
    }
}

impl<T: Clone + Send + Sync + 'static> Pattern<T> {
    /// Build a pattern from a raw query function. Most code should reach for the
    /// combinators in [`crate::combinators`] instead.
    pub fn new(query: impl Fn(TimeSpan) -> Vec<Hap<T>> + Send + Sync + 'static) -> Self {
        Pattern {
            query: Arc::new(query),
        }
    }

    /// Query the pattern over `span`, returning the events that fall in it.
    pub fn query(&self, span: TimeSpan) -> Vec<Hap<T>> {
        (self.query)(span)
    }

    /// Sample the value at an instant (zero-width query). Used to read a
    /// patternised control parameter at a given hap's onset.
    pub fn value_at(&self, t: Time) -> Option<T> {
        self.query(TimeSpan::new(t, t))
            .into_iter()
            .next()
            .map(|h| h.value)
    }

    // ── Internal query helpers ──────────────────────────────────────────────
    // These are the primitives every combinator is composed from.

    /// Wrap the query so it is split at integer cycle boundaries first. Required
    /// by any pattern whose per-query logic assumes a single cycle.
    pub fn split_queries(self) -> Pattern<T> {
        Pattern::new(move |span| {
            span.split_cycles()
                .into_iter()
                .flat_map(|s| self.query(s))
                .collect()
        })
    }

    /// Transform the **query** span's time before querying (does not touch
    /// results). Pair with [`with_hap_time`](Self::with_hap_time) to time-warp.
    pub fn with_query_time(self, f: impl Fn(Time) -> Time + Send + Sync + 'static) -> Pattern<T> {
        Pattern::new(move |span| self.query(span.with_time(&f)))
    }

    /// Transform the **query** span as a whole (e.g. reflect it).
    pub fn with_query_span(
        self,
        f: impl Fn(TimeSpan) -> TimeSpan + Send + Sync + 'static,
    ) -> Pattern<T> {
        Pattern::new(move |span| self.query(f(span)))
    }

    /// Transform every **result** time coordinate (`whole` and `part`).
    pub fn with_hap_time(self, f: impl Fn(Time) -> Time + Send + Sync + 'static) -> Pattern<T> {
        Pattern::new(move |span| {
            self.query(span)
                .into_iter()
                .map(|h| h.map_time(&f))
                .collect()
        })
    }

    /// Transform every **result** span as a whole.
    pub fn with_hap_span(
        self,
        f: impl Fn(TimeSpan) -> TimeSpan + Send + Sync + 'static,
    ) -> Pattern<T> {
        Pattern::new(move |span| {
            self.query(span)
                .into_iter()
                .map(|mut h| {
                    h.whole = h.whole.map(&f);
                    h.part = f(h.part);
                    h
                })
                .collect()
        })
    }

    /// Map every value, producing a pattern of a new payload type (functor map).
    pub fn fmap<U: Clone + Send + Sync + 'static>(
        self,
        f: impl Fn(T) -> U + Send + Sync + 'static,
    ) -> Pattern<U> {
        Pattern::new(move |span| {
            self.query(span)
                .into_iter()
                .map(|h| h.map_value(&f))
                .collect()
        })
    }

    /// Keep only the haps matching `pred`.
    pub fn filter_haps(
        self,
        pred: impl Fn(&Hap<T>) -> bool + Send + Sync + 'static,
    ) -> Pattern<T> {
        Pattern::new(move |span| {
            self.query(span).into_iter().filter(|h| pred(h)).collect()
        })
    }

    /// Keep only haps whose onset falls inside the query (drops tail fragments
    /// and continuous samples). Essential before reasoning about "events".
    pub fn filter_onsets(self) -> Pattern<T> {
        self.filter_haps(|h| h.has_onset())
    }

    /// Stamp `span` onto every hap that doesn't already carry one.
    ///
    /// The language layer (Fase 1) builds each mini-notation leaf as a pattern
    /// and tags it so every event points back at the exact source bytes for live
    /// highlight. An inner leaf's own span is preserved (it wins over an outer
    /// container's), so tagging a group only fills the gaps.
    pub fn tag_span(self, span: SourceSpan) -> Pattern<T> {
        Pattern::new(move |q| {
            self.query(q)
                .into_iter()
                .map(|mut h| {
                    if h.span.is_none() {
                        h.span = Some(span);
                    }
                    h
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::{pure, silence};

    #[test]
    fn pure_repeats_once_per_cycle() {
        let p = pure(42);
        let haps = p.query(TimeSpan::new(Time::ZERO, Time::int(2)));
        assert_eq!(haps.len(), 2);
        assert!(haps.iter().all(|h| h.value == 42 && h.has_onset()));
        assert_eq!(haps[0].whole, Some(TimeSpan::cycle(0)));
        assert_eq!(haps[1].whole, Some(TimeSpan::cycle(1)));
    }

    #[test]
    fn silence_is_empty() {
        let p: Pattern<i32> = silence();
        assert!(p.query(TimeSpan::new(Time::ZERO, Time::int(4))).is_empty());
    }

    #[test]
    fn value_at_samples_an_instant() {
        let p = pure("x");
        assert_eq!(p.value_at(Time::new(1, 2)), Some("x"));
    }

    #[test]
    fn fmap_changes_payload() {
        let p = pure(3).fmap(|n| n * 10);
        assert_eq!(p.value_at(Time::ZERO), Some(30));
    }

    #[test]
    fn tag_span_fills_only_missing_spans() {
        use crate::span::SourceSpan;
        let outer = SourceSpan::new(0, 10);
        let inner = SourceSpan::new(2, 4);
        // A hap that already carries a span keeps it; a bare one gets `outer`.
        let p = pure(1)
            .tag_span(inner) // inner wins where present
            .tag_span(outer); // fills the rest
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert_eq!(h.span, Some(inner));

        let bare = pure(1).tag_span(outer);
        assert_eq!(bare.query(TimeSpan::cycle(0))[0].span, Some(outer));
    }
}
