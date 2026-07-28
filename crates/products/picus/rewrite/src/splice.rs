//! Editing a file by **replacing byte ranges of what is already there**.
//!
//! This is the single most important decision in the crate, so it is stated
//! plainly: Picus never re-prints a file. It takes the original text, replaces the
//! ranges it means to change, and leaves every other byte exactly as it found it.
//!
//! The alternative — parse the file, modify a tree, print it back — is how a tool
//! reformats four thousand lines nobody asked it to touch, turns a review into
//! noise, and eventually loses something it did not understand. Splicing makes the
//! byte-identical round trip a **property of the algorithm** rather than a quality
//! to be tested for: with no splices, the output is the input, and there is no
//! code path by which it could be otherwise.

use std::ops::Range;

use crate::error::RewriteError;

/// One replacement: what to take out, and what to put in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    /// Byte range into the original text. An empty range is an insertion.
    pub range: Range<usize>,
    pub replacement: String,
    /// What this edit is, for the diff header and the log. Written for a person
    /// reviewing the change, not for a machine.
    pub reason: String,
}

impl Splice {
    /// Insert text at a point, replacing nothing.
    pub fn insert(at: usize, text: impl Into<String>, reason: impl Into<String>) -> Splice {
        Splice { range: at..at, replacement: text.into(), reason: reason.into() }
    }

    /// Replace an existing range — regenerating a block Picus wrote before.
    pub fn replace(
        range: Range<usize>,
        text: impl Into<String>,
        reason: impl Into<String>,
    ) -> Splice {
        Splice { range, replacement: text.into(), reason: reason.into() }
    }

    /// Is this a pure insertion?
    pub fn is_insertion(&self) -> bool {
        self.range.is_empty()
    }
}

/// Apply every splice to `original`, leaving everything else byte-identical.
///
/// The splices are validated before a single byte is copied: out of bounds, off a
/// character boundary, or overlapping each other are all refused up front. A
/// partially applied edit is not a thing this function can produce.
pub fn apply_splices(original: &str, splices: &[Splice]) -> Result<String, RewriteError> {
    if splices.is_empty() {
        // Stated explicitly rather than falling out of the loop below, because
        // "no splices means the input unchanged" is the invariant the whole crate
        // rests on and it deserves to be visible.
        return Ok(original.to_string());
    }

    let mut ordered: Vec<&Splice> = splices.iter().collect();
    ordered.sort_by_key(|s| (s.range.start, s.range.end));
    validate(original, &ordered)?;

    let mut out = String::with_capacity(original.len() + added_length(&ordered));
    let mut cursor = 0usize;
    for splice in &ordered {
        out.push_str(&original[cursor..splice.range.start]);
        out.push_str(&splice.replacement);
        cursor = splice.range.end;
    }
    out.push_str(&original[cursor..]);
    Ok(out)
}

fn validate(original: &str, ordered: &[&Splice]) -> Result<(), RewriteError> {
    let mut previous_end: Option<usize> = None;
    for splice in ordered {
        let Range { start, end } = splice.range;
        if start > end || end > original.len() {
            return Err(RewriteError::SpliceOutOfBounds {
                range: start..end,
                length: original.len(),
            });
        }
        if !original.is_char_boundary(start) || !original.is_char_boundary(end) {
            return Err(RewriteError::SpliceOffBoundary { range: start..end });
        }
        if let Some(previous_end) = previous_end {
            // Touching is fine — two insertions at the same point, or a
            // replacement ending exactly where the next begins. Overlapping is
            // not: the result would depend on the order the edits were listed in,
            // and an edit whose outcome depends on list order is a bug waiting
            // for a bigger repository.
            if start < previous_end {
                return Err(RewriteError::SplicesOverlap {
                    first: previous_end,
                    second: start,
                });
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

fn added_length(ordered: &[&Splice]) -> usize {
    ordered.iter().map(|s| s.replacement.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "-- header\nINSERT INTO A VALUES (1);\nINSERT INTO B VALUES (2);\n";

    fn splice(range: Range<usize>, text: &str) -> Splice {
        Splice::replace(range, text, "test")
    }

    #[test]
    fn no_splices_means_the_input_unchanged() {
        // The invariant the whole crate rests on.
        assert_eq!(apply_splices(SOURCE, &[]).unwrap(), SOURCE);
    }

    #[test]
    fn an_insertion_leaves_everything_around_it_untouched() {
        let out = apply_splices(SOURCE, &[Splice::insert(10, "-- added\n", "test")]).unwrap();
        assert_eq!(out, "-- header\n-- added\nINSERT INTO A VALUES (1);\nINSERT INTO B VALUES (2);\n");
    }

    #[test]
    fn several_edits_apply_in_position_order_not_list_order() {
        // Listing them backwards must produce the same file: otherwise the caller
        // has to know an ordering rule, and one day it will not.
        let forwards = apply_splices(SOURCE, &[
            Splice::insert(0, "-- first\n", "a"),
            Splice::insert(10, "-- second\n", "b"),
        ])
        .unwrap();
        let backwards = apply_splices(SOURCE, &[
            Splice::insert(10, "-- second\n", "b"),
            Splice::insert(0, "-- first\n", "a"),
        ])
        .unwrap();
        assert_eq!(forwards, backwards);
        assert!(forwards.starts_with("-- first\n-- header\n-- second\n"));
    }

    #[test]
    fn a_replacement_swaps_exactly_its_range() {
        let start = SOURCE.find("INSERT INTO A VALUES (1);").unwrap();
        let out = splice_at(start, "INSERT INTO A VALUES (1);".len(), "DELETE FROM A;");
        assert_eq!(out, "-- header\nDELETE FROM A;\nINSERT INTO B VALUES (2);\n");
    }

    fn splice_at(start: usize, len: usize, text: &str) -> String {
        apply_splices(SOURCE, &[splice(start..start + len, text)]).unwrap()
    }

    #[test]
    fn overlapping_edits_are_refused_rather_than_resolved() {
        let err = apply_splices(SOURCE, &[splice(0..20, "x"), splice(10..30, "y")]).unwrap_err();
        assert!(matches!(err, RewriteError::SplicesOverlap { .. }));
    }

    #[test]
    fn touching_edits_are_allowed() {
        // A replacement ending exactly where the next begins is unambiguous.
        let out = apply_splices(SOURCE, &[splice(0..9, "-- one"), splice(9..10, "!\n")]).unwrap();
        assert!(out.starts_with("-- one!\n"));
    }

    #[test]
    fn a_range_past_the_end_is_refused() {
        let err = apply_splices(SOURCE, &[splice(0..SOURCE.len() + 1, "x")]).unwrap_err();
        assert!(matches!(err, RewriteError::SpliceOutOfBounds { .. }));
        let err = apply_splices(SOURCE, &[splice(5..2, "x")]).unwrap_err();
        assert!(matches!(err, RewriteError::SpliceOutOfBounds { .. }));
    }

    #[test]
    fn a_range_that_cuts_a_character_in_half_is_refused() {
        // These files hold accented text; an offset computed against bytes and
        // used against characters has to fail loudly, not truncate silently.
        let source = "-- soglia già applicata\n";
        let cut = source.find('à').unwrap() + 1;
        let err = apply_splices(source, &[splice(cut..cut + 1, "x")]).unwrap_err();
        assert!(matches!(err, RewriteError::SpliceOffBoundary { .. }));
    }

    #[test]
    fn two_insertions_at_the_same_point_both_land() {
        let out = apply_splices(SOURCE, &[
            Splice::insert(10, "-- a\n", "a"),
            Splice::insert(10, "-- b\n", "b"),
        ])
        .unwrap();
        assert!(out.contains("-- a\n-- b\n"));
    }

    #[test]
    fn deleting_is_replacing_with_nothing() {
        let start = SOURCE.find("INSERT INTO B VALUES (2);\n").unwrap();
        let out = apply_splices(SOURCE, &[splice(start..SOURCE.len(), "")]).unwrap();
        assert_eq!(out, "-- header\nINSERT INTO A VALUES (1);\n");
    }
}
