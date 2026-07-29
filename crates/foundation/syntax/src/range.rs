//! [`ByteRange`] — a half-open span of the source, in bytes.

use serde::{Deserialize, Serialize};

/// `[start, end)` in **bytes**, not characters and not UTF-16 units.
///
/// Bytes because that is what Tree-sitter reports and what slicing a `str`
/// takes; a conversion anywhere in between is a place for an off-by-one to hide
/// in a file with accents in it. Frontends that need character offsets convert
/// once, at the edge, against the same string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// The slice this range names, or `None` when it does not land on character
    /// boundaries of `source` — which means it came from a different string.
    pub fn slice<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }

    /// Do these two ranges share a byte? Touching at an edge does not count.
    pub fn overlaps(&self, other: &ByteRange) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Is `other` entirely inside this range? A range contains itself.
    pub fn contains(&self, other: &ByteRange) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

impl From<std::ops::Range<usize>> for ByteRange {
    fn from(r: std::ops::Range<usize>) -> Self {
        Self { start: r.start, end: r.end }
    }
}

impl From<ByteRange> for std::ops::Range<usize> {
    fn from(r: ByteRange) -> Self {
        r.start..r.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_ranges_do_not_overlap() {
        // The distinction the edit applier rests on: two edits that meet at a
        // byte are both applicable, two that share one are a conflict.
        let a = ByteRange::new(0, 5);
        let b = ByteRange::new(5, 9);
        assert!(!a.overlaps(&b));
        assert!(a.overlaps(&ByteRange::new(4, 6)));
    }

    #[test]
    fn a_range_off_a_character_boundary_slices_to_nothing() {
        let text = "città";
        // `à` occupies bytes 4 and 5, so 4..5 cuts it in half.
        assert_eq!(ByteRange::new(4, 5).slice(text), None);
        assert_eq!(ByteRange::new(0, 4).slice(text), Some("citt"));
        assert_eq!(ByteRange::new(0, 6).slice(text), Some(text));
    }
}
