//! [`SelectShape`] — the little a caller needs to know about a SELECT's projection
//! in order to add a column to it.
//!
//! Picus injects a hidden row key into a read whose large objects it wants to mask
//! but whose key the query did not select. That injection is the *only* reason this
//! exists, so it captures exactly three things and nothing more: where the new item
//! goes, whether the key is already there (`*`, or a plain column of that name), and
//! whether adding a column would change the result — in which case it must not.
//!
//! Everything here errs one way on purpose: when the shape is anything the walk is
//! not sure how to read, [`SelectShape::not_injectable`] is `true`. Refusing to
//! inject only costs the value being shown instead of a size; injecting into a
//! shape that could not take it would change what the user's query returns.

use serde::Serialize;

/// What a top-level SELECT projects, insofar as it bears on splicing a column in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SelectShape {
    /// Byte offset just past the last projection item — where a new one is spliced
    /// in, immediately before the `FROM`.
    pub select_list_end: usize,
    /// A `*` or `alias.*` is projected, so every column of the single source — the
    /// key included — is already in the result.
    pub star: bool,
    /// The folded output names of the top-level projection: an item's alias, or the
    /// trailing identifier of a plain column reference. An expression contributes
    /// nothing, because nothing it produces can match a key column by name.
    pub outputs: Vec<String>,
    /// The projection is not a plain per-row column list — `DISTINCT`, `GROUP BY`,
    /// `HAVING`, a set operation, or any computed/aggregated item — so adding a
    /// column could change the result or be rejected by the server. When this is
    /// `true` the caller does not inject and falls back to reading the value whole.
    pub not_injectable: bool,
}
