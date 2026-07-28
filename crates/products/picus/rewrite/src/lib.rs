//! `picus-rewrite` — writing generated SQL into files that already exist, without
//! changing anything else about them.
//!
//! This is the only crate in Picus that modifies a user's repository, and it is
//! built around one refusal: **Picus never re-prints a file.** It replaces the byte
//! ranges it means to change and leaves every other byte exactly as it found it.
//! That makes the byte-identical round trip a property of the algorithm rather
//! than a quality to test for — with no edits, the output *is* the input, and
//! there is no code path by which it could be otherwise.
//!
//! Three things follow from that, and they are the crate:
//!
//! * [`source::SourceText`] keeps the original bytes next to the decoded text, so
//!   "can this file be written back exactly as found?" is **checked before any
//!   edit is prepared**. A file Picus cannot reproduce — a mis-detected encoding,
//!   a lossy decode — is one it refuses to write to at all.
//! * [`splice`] applies non-overlapping range replacements in position order, so
//!   the result never depends on the order the caller listed its edits in.
//! * [`apply`] splits the work into `prepare` (everything fallible except writing)
//!   and `commit` (writing, and nothing else). What `prepare` produces is the exact
//!   bytes that will land, which is also what the diff preview shows — the user
//!   reviews the real thing. A failure part-way through `commit` puts every file
//!   already written back.
//!
//! Line endings are converted to the destination file's own on the way in:
//! generated SQL arrives with `\n`, half these repositories are CRLF, and mixed
//! endings turn a three-line addition into a whole-file diff.
//!
//! ## Public API: use the [`prelude`]

pub mod apply;
pub mod error;
pub mod prelude;
pub mod source;
pub mod splice;
