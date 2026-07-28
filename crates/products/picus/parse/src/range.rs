//! [`ByteRange`] — the crate's only way of referring to source text.
//!
//! **Byte offsets, never char offsets.** Every other Arbor crate that touches
//! text uses UTF-8 byte offsets, Tree-sitter reports byte offsets natively, and
//! the rewriter that consumes this crate splices bytes. Converting to characters
//! anywhere in between would be a lossy round trip waiting to happen.
//!
//! Nothing in this crate stores the source. A `ParsedFile` is a map of a string
//! the caller already owns — which is what makes a byte-identical rewrite
//! possible, and what keeps a parsed file cheap to hold.

use serde::{Deserialize, Serialize};

/// A half-open byte range `[start, end)` into the parsed source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// The bytes this range covers.
    ///
    /// Returns `""` rather than panicking when the range is out of bounds or
    /// lands inside a multi-byte character. A parser is handed hostile and
    /// truncated files by definition; it must never be the thing that panics.
    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        source.get(self.start..self.end).unwrap_or("")
    }
}

/// 1-based line and column (column counted in bytes) for a byte offset.
///
/// **Linear in the offset**, because it counts newlines from byte zero. That is
/// fine for a caller asking once and catastrophic for one asking per statement:
/// doing it that way made indexing a real repository quadratic in the size of
/// each file, and took five minutes over eleven megabytes.
///
/// So this is for callers that have a source and no parse. Anything holding a
/// [`ParsedFile`](crate::statement::ParsedFile) must use its
/// `line_col_at` / `line_of`, which binary-search an index built once.
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let before = &source.as_bytes()[..clamped];
    let line = before.iter().filter(|b| **b == b'\n').count() + 1;
    let line_start = before.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    (line, clamped - line_start + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_never_panics_on_a_bad_range() {
        let src = "città";
        // 5 lands inside the two-byte `à`.
        assert_eq!(ByteRange::new(0, 5).slice(src), "");
        assert_eq!(ByteRange::new(0, 900).slice(src), "");
        assert_eq!(ByteRange::new(0, 4).slice(src), "citt");
    }

    #[test]
    fn line_col_is_one_based_and_clamped() {
        let src = "a\nbc\n";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 2), (2, 1));
        assert_eq!(line_col(src, 3), (2, 2));
        assert_eq!(line_col(src, 999), (3, 1));
    }
}
