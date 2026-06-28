//! The crate's tiny error surface.
//!
//! The pattern algebra is essentially infallible — a query always returns. The
//! only fallible operation is parsing a human-written scale spec
//! (`"c:minor"`), so the error type stays minimal. Keeping it here (rather than
//! leaning on `Option`) honours the workspace rule that a library crate depends
//! only on its own `error`/base types and can be split off cleanly later.

use std::fmt;

/// Errors produced by `merula-pattern`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    /// A scale spec was well-formed but names an unknown mode.
    UnknownScale(String),
    /// A scale spec was malformed (expected `"<root>:<mode>"`).
    BadScaleSpec(String),
    /// A note name could not be parsed.
    UnknownNote(String),
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternError::UnknownScale(s) => write!(f, "unknown scale mode: {s}"),
            PatternError::BadScaleSpec(s) => {
                write!(f, "bad scale spec {s:?} (expected \"<root>:<mode>\")")
            }
            PatternError::UnknownNote(s) => write!(f, "unknown note name: {s}"),
        }
    }
}

impl std::error::Error for PatternError {}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, PatternError>;
