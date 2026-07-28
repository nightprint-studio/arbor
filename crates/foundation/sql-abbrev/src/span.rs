//! Byte spans, and the "slot" every grammar position is recorded as.
//!
//! The parser records a slot for a position the grammar *allows*, whether or not
//! anything has been typed there yet. That is what makes one parser serve both
//! jobs: `s#ordini>` has an empty table slot at offset 9, so asking what is under
//! the caret at 9 has an answer, and the answer comes from the same parse the
//! expansion would use.

use serde::{Deserialize, Serialize};

/// A byte range in the abbreviation the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// A position where something may be typed and nothing has been.
    pub fn empty(at: usize) -> Self {
        Self { start: at, end: at }
    }

    /// Does the caret sit in this slot? **Both ends inclusive.**
    ///
    /// The inclusive end is the whole point: a caret immediately after the last
    /// character of `loc` is what "completing `loc`" means, and an exclusive end
    /// would answer `None` for every completion anyone ever asks for.
    pub fn holds(self, cursor: usize) -> bool {
        cursor >= self.start && cursor <= self.end
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// Text at a position, either of which may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub text: String,
    pub span: Span,
}

impl Slot {
    pub fn new(text: impl Into<String>, span: Span) -> Self {
        Self { text: text.into(), span }
    }

    pub fn empty(at: usize) -> Self {
        Self { text: String::new(), span: Span::empty(at) }
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// What has been typed in this slot *up to* the caret — what a completion
    /// list filters on.
    ///
    /// Clamped and walked back to a character boundary rather than sliced blind:
    /// these are accented-text databases, the caret arrives from an editor that
    /// may count differently, and a panic in a completion handler takes the
    /// editor's keystroke with it.
    pub fn prefix_to(&self, cursor: usize) -> String {
        let mut offset = cursor.saturating_sub(self.span.start).min(self.text.len());
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        self.text[..offset].to_string()
    }
}

/// Move a caret back to the nearest character boundary at or before it, and clamp
/// it into the string. Every public entry point that takes a caret runs it first.
pub fn clamp_to_boundary(input: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(input.len());
    while cursor > 0 && !input.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_holds_both_of_its_ends() {
        let span = Span::new(2, 5);
        assert!(span.holds(2), "the caret before the first character is in the slot");
        assert!(span.holds(5), "…and so is the caret after the last one");
        assert!(!span.holds(1));
        assert!(!span.holds(6));
    }

    #[test]
    fn an_empty_slot_still_holds_its_position() {
        // `s#ordini>` — nothing typed after the arrow, but that position is where
        // the answer has to come from.
        assert!(Span::empty(9).holds(9));
        assert!(Span::empty(9).is_empty());
    }

    #[test]
    fn a_prefix_never_splits_a_character() {
        // "città" — the caret arrives mid-character; it walks back rather than
        // panicking inside a completion handler.
        let slot = Slot::new("città", Span::new(0, "città".len()));
        assert_eq!(slot.prefix_to(4), "citt");
        assert_eq!(slot.prefix_to(5), "citt", "byte 5 is inside `à`");
        assert_eq!(slot.prefix_to(6), "città");
        assert_eq!(slot.prefix_to(999), "città", "past the end clamps to all of it");
        assert_eq!(slot.prefix_to(0), "");
    }

    #[test]
    fn a_caret_is_clamped_into_the_input() {
        assert_eq!(clamp_to_boundary("s#loc", 99), 5);
        assert_eq!(clamp_to_boundary("s#città", 6), 6);
        assert_eq!(clamp_to_boundary("s#città", 7), 6, "inside `à` walks back");
    }
}
