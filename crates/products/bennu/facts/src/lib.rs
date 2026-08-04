//! `bennu-facts` — the Java facts a **framework extension** reads, and the rule for deciding
//! whether an annotation is the one you think it is.
//!
//! ## Why this crate exists
//!
//! `bennu-spring` was the first framework extension, and it grew two things that turned out
//! not to be about Spring at all:
//!
//! - a **tree-sitter scan** producing annotation-shaped facts — types, fields, methods,
//!   parameters, and every annotation argument with its byte span ([`scan`]);
//! - the **origin rule** that resolves `@Service` (or `@Entity`, or `@Query`) through the
//!   file's imports the way the compiler would, so a project's own annotation of the same name
//!   is never mistaken for the framework's ([`origin`]).
//!
//! The second extension needs both, identically. The three ways to give it them were: depend on
//! `bennu-spring` (backwards — JPA exists without Spring), copy the scanner (a file that will
//! keep changing, duplicated), or extract. This is the extraction.
//!
//! ## What stayed behind
//!
//! **Policy.** Which markers make a file worth parsing, which packages an annotation may come
//! from, what a bean is — all of that is per-framework and lives in the extension. This crate
//! provides the mechanism and the shape of the answer: [`mentions_any`] takes the caller's
//! markers, [`AnnotationTable`] holds the caller's packages.
//!
//! A leaf: tree-sitter and nothing else. No Bennu dependency, so an extension built on it stays
//! as portable as the seam promises.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_facts::prelude::...`.

// The annotation-origin rule + the per-framework table it reads.
pub mod origin;
pub mod prelude;
// The tree-sitter pass and the facts it yields.
pub mod scan;
