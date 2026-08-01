//! [`Span`] — where a node came from in the source text.
//!
//! **UTF-8 byte offsets**, half-open (`start..end`). Not char indices and not
//! line/column pairs: every consumer of a span ends up slicing the original
//! `&str` (the editor to decorate a wikilink, the index to record where a tag
//! occurred, a refactor to splice a subtree back out), and byte offsets are the
//! only unit that does that without a conversion table. Tree-sitter reports byte
//! offsets natively, so the reader stays a copy rather than a translation.

use serde::{Deserialize, Serialize};

/// A half-open byte range `start..end` into the source the document was read from.
///
/// Spans are only meaningful against *that* source string. A document built by
/// hand (a template, a refactor result) may carry [`Span::EMPTY`] everywhere,
/// which is why nothing downstream may treat a span as proof of provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Span {
    /// First byte of the node, inclusive.
    pub start: usize,
    /// One past the last byte of the node, exclusive.
    pub end: usize,
}

impl Span {
    /// The zero-width span at offset 0 — the marker for "synthesised, not read".
    pub const EMPTY: Span = Span { start: 0, end: 0 };

    /// A span covering `start..end`.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Length in bytes. Saturating, so an inverted span reads as empty rather
    /// than panicking in a release build far from the reader that produced it.
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span covers no bytes.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether `offset` falls inside the span (half-open: `end` is outside).
    pub const fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// The source text this span covers, or `None` when the span does not land on
    /// character boundaries of `source` — which is what happens when a span from
    /// one document is applied to another.
    pub fn slice<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }

    /// The smallest span covering both. Used when a node's extent is assembled
    /// from its children rather than reported directly by the grammar.
    pub fn join(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_utf8_by_byte_offset() {
        let src = "città [[nota]]";
        // "città" is 6 bytes, not 5 — the whole reason spans are byte offsets.
        let span = Span::new(0, 6);
        assert_eq!(span.slice(src), Some("città"));
        assert_eq!(span.len(), 6);
    }

    #[test]
    fn slice_off_a_char_boundary_is_none_not_a_panic() {
        let src = "città";
        assert_eq!(Span::new(0, 5).slice(src), None);
    }

    #[test]
    fn contains_is_half_open() {
        let span = Span::new(2, 5);
        assert!(span.contains(2));
        assert!(span.contains(4));
        assert!(!span.contains(5));
    }

    #[test]
    fn join_covers_both() {
        assert_eq!(Span::new(2, 4).join(Span::new(9, 12)), Span::new(2, 12));
    }

    #[test]
    fn inverted_span_reads_as_empty() {
        assert!(Span::new(9, 2).is_empty());
    }
}
