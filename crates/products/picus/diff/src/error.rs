//! The few ways a comparison can refuse to run.
//!
//! Every variant here is a **caller** mistake or a driver that broke its own
//! contract — never a difference between the two databases. Differences are the
//! return value; this is the channel for "the question as asked has no answer",
//! and it exists so the answer is never a comparison quietly done over the wrong
//! columns.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Which of the two things being compared a problem is on.
///
/// `A` and `B` and not "left/right" or "source/target": the report labels them
/// that way end to end, and a message that renamed them mid-flight would be one
/// more thing for a reader to map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    A,
    B,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::B => write!(f, "B"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DiffError {
    /// The row match was asked for on a column one of the two sides does not
    /// have. Comparing anyway would mean matching on the columns that happen to
    /// exist, which is a different question with the same-looking answer.
    #[error("`{label}`: cannot match rows on `{column}` — side {side} has no such column")]
    MissingKeyColumn { label: String, column: String, side: Side },

    /// A column named in the per-table `columns` list is not in the result.
    #[error("`{label}`: column `{column}` was asked for but side {side} does not have it")]
    MissingColumn { label: String, column: String, side: Side },

    /// A row whose width does not match the header. Only a broken driver
    /// produces one, and indexing into it would either panic or compare two
    /// unrelated cells.
    #[error(
        "`{label}`: row {row} on side {side} has {found} values for {expected} columns"
    )]
    RowWidthMismatch {
        label: String,
        side: Side,
        row: usize,
        expected: usize,
        found: usize,
    },
}
