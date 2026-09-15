//! `bennu-sheet` — a spreadsheet in a project tree, read into a grid.
//!
//! ## Why this exists at all
//!
//! A legacy enterprise repository is full of them: the column mapping an import expects, the
//! translation table somebody sent, the test fixtures a batch job reads. Opening one meant leaving
//! the editor, and a file the editor refuses to show is a file you stop checking.
//!
//! ## Where the line is drawn
//!
//! **Reading the container is not ours.** `.xls`, `.xlsx`, `.xlsb` and `.ods` are four unrelated
//! binary formats, and the failure mode of getting one slightly wrong is not an error — it is a
//! column of plausible nonsense. [`calamine`] does that part, tested against real files.
//!
//! **What a cell READS AS is ours**, and it is all of [`model`]: which serial numbers are dates and
//! how a date is spelled, how a float is printed so a column of counts does not read as
//! measurements, how much of a sheet is worth drawing. Those are decisions about a viewer, they are
//! the same for every format, and they are where the tests are.
//!
//! ## What it deliberately is not
//!
//! **A spreadsheet.** No formula evaluation, no styling, no charts, no images. A formula's *last
//! computed value* is shown, because that is what the file records and what a reader is checking;
//! `=SUM(A1:A9)` is not recomputed, and a cell whose value the writer never stored comes back
//! empty rather than guessed at.
//!
//! **Writable.** Nothing here writes. The viewer over it is a preview, so the file never enters a
//! buffer that a stray Ctrl+S could put back.
//!
//! ## Public API: use the [`prelude`]

pub mod model;
pub mod prelude;
pub mod reader;

pub use reader::read;
