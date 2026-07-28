//! Parse errors as **data**.
//!
//! Parsing never fails as a whole and never returns `Result`: Tree-sitter always
//! produces a tree, and the statements it did understand are still worth having
//! — an upgrade script with one bad line should still yield its other twelve
//! statements to the inventory. So an error is a located record attached to the
//! file, and the caller decides how loud to be about it.

use serde::{Deserialize, Serialize};

use crate::range::ByteRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseErrorKind {
    /// Text the grammar could not fit anywhere.
    Syntax,
    /// A token the grammar required and the source did not have. Tree-sitter
    /// inserts it as a zero-width node, so the range is empty and marks the
    /// position where it should have been.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub range: ByteRange,
    /// The kind of node the error sits inside — `select_core`, `value_row`,
    /// `source_file` … Context, so a message can say *where* rather than only
    /// *which line*.
    pub parent: String,
    /// The offending source text, truncated. Empty for a `Missing` error, which
    /// has no text by construction; `expected` carries the name instead.
    pub text: String,
    /// For `Missing`, the node kind that was expected (`";"`, `")"`, …).
    pub expected: Option<String>,
}

/// How much offending text a `ParseError` carries. Long enough to recognise the
/// statement, short enough that a hostile one-line file cannot make the error
/// list bigger than the file.
pub(crate) const ERROR_TEXT_LIMIT: usize = 120;
