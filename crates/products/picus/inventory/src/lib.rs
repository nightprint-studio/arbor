//! `picus-inventory` — object → where it lives → how well each branch covers it.
//!
//! One index over a whole script repository, answering the question the product
//! exists to ask: *does the other dialect's branch do this too?* Every object
//! gets a row, every branch/folder gets a column, and the cell is the number of
//! statements that touch the object there. **`0` is the interesting value.**
//!
//! Pure: parsed files in, an index out. Reading the files, decoding them and
//! parsing them belongs to the caller — which is what keeps this crate free of
//! the filesystem and cheap to run on an editor buffer that has not been saved.
//!
//! See `README.md` for the identifier-folding rule, which is the one decision
//! everything else here depends on.

pub mod build;
pub mod entry;
pub mod input;
pub mod kind;
pub mod prelude;
pub mod wire;

#[cfg(test)]
mod testing;
