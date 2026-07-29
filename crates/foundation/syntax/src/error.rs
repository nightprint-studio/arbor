//! What can go wrong, in the user's terms.
//!
//! No `thiserror` here: this crate has four failure modes and a hand-written
//! `Display` says more useful things about a bad pattern than a derive would.
//! Every message is written to be shown to somebody typing a pattern into a box,
//! because that is where all four of them come from.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxError {
    /// The grammar would not load. In practice a `tree-sitter` version mismatch
    /// between the crate and the compiled grammar.
    Language(String),
    /// A placeholder was opened and never closed, or named something unusable.
    Placeholder(String),
    /// The pattern text does not parse as the target language, even with its
    /// placeholders standing in for names.
    ///
    /// Carries the pattern's own bytes at the failure so the caller can point at
    /// it — a pattern box that says "syntax error" and nothing else is a box you
    /// give up on.
    Pattern { reason: String, at: Option<crate::range::ByteRange> },
    /// A replacement template names a placeholder the pattern never captured, or
    /// indexes past the end of a list capture.
    Template(String),
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyntaxError::Language(what) => write!(f, "the grammar could not be loaded: {what}"),
            SyntaxError::Placeholder(what) => write!(f, "{what}"),
            SyntaxError::Pattern { reason, .. } => write!(f, "{reason}"),
            SyntaxError::Template(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for SyntaxError {}
