//! Canonical entry point for `bennu-sheet`'s public API.
//!
//! A host needs two things: the reader that picks its own format, and the shape of what comes back.

pub use crate::model::{Cell, CellKind, Sheet, Workbook, MAX_COLS, MAX_ROWS, MAX_SHEETS};
pub use crate::read;
