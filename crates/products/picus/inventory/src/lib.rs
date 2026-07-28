//! `picus-inventory` — object → where it lives → how well each folder covers it.
//!
//! One index over a whole script repository, answering the question the product
//! exists to ask: *do the other dialect's scripts do this too?* Every object gets
//! a row, every folder that holds scripts gets a column — keyed by its
//! project-relative path — and the cell is the number of statements that touch
//! the object there. **`0` is the interesting value.**
//!
//! A column is a folder rather than a dialect on purpose. The folder is where the
//! statement actually is, and it is the only key that survives a repository with
//! eleven folders called `ORA`; grouping those columns back into "what Oracle
//! does" is the reader's question, not the index's, and the resolved tree answers
//! it without this crate having to guess which grouping anybody wanted.
//!
//! Pure: parsed files in, an index out. Reading the files, decoding them and
//! parsing them belongs to the caller — which is what keeps this crate free of
//! the filesystem and cheap to run on an editor buffer that has not been saved.
//!
//! See `README.md` for the identifier-folding rule, which is the one decision
//! everything else here depends on.

pub mod build;
pub mod builtin;
pub mod entry;
pub mod input;
pub mod kind;
pub mod prelude;
pub mod wire;

#[cfg(test)]
mod testing;
