//! # arbor-syntax
//!
//! Two things you can do with a Tree-sitter tree once you stop caring which
//! language produced it: **look at it**, and **rewrite it by shape**.
//!
//! ## It knows no language, on purpose
//!
//! Nothing here names a keyword, a node kind or a file extension. The caller
//! brings a [`tree_sitter::Language`] — Picus its SQL grammar, Bennu its Java
//! one — and everything below works the same for both. That is not future-proofing
//! for its own sake: an outline panel and a structural replace are the same two
//! features in every editor that has them, and writing them twice would be two
//! sets of bugs about the same thing.
//!
//! ## What it hands back
//!
//! Byte ranges into a string the caller still owns, exactly like `picus-parse`
//! and for the same reason: this crate never stores the source and never
//! reconstructs it, so a click on a node can select the real bytes and an edit
//! can be spliced with the rest of the file surviving untouched.
//!
//! ## The two halves
//!
//! * [`outline`] — the tree as data: kinds, fields, ranges, errors, one node per
//!   node, with the depth and size limits a UI needs to stay honest about a file
//!   with a hundred thousand nodes.
//! * [`pattern`] — structural search and replace. A pattern is **source text of
//!   the target language** with placeholders in it, so there is no second syntax
//!   to learn: `INSERT INTO $t$ ($cols...$) VALUES ($vals...$)` is SQL that
//!   happens to have holes. It is parsed with the same grammar as the subject,
//!   which is what makes the match structural rather than textual — a match
//!   survives reformatting, comments and line breaks.
//!
//! ```no_run
//! use arbor_syntax::prelude::*;
//!
//! # fn demo(language: tree_sitter::Language) -> Result<(), SyntaxError> {
//! let source = "class A { int x = f(1, 2); }";
//! let pattern = Pattern::compile(&language, "f($args...$)")?;
//! for found in pattern.find_all(&language, source)? {
//!     println!("{}", found.range.slice(source).unwrap_or(""));
//!     println!("{:?}", found.capture("args").map(|c| c.range.slice(source)));
//! }
//! # Ok(())
//! # }
//! ```

pub mod edit;
pub mod error;
pub mod outline;
pub mod pattern;
pub mod prelude;
pub mod range;
