//! The Java formatter.
//!
//! A token-level engine (see [`engine`]) that works on text, not on a parse tree, so it still
//! formats a file that does not compile — half-written code is exactly when a formatter is wanted.
//! Everything it can do is a field of [`FormatStyle`]; the default style changes nothing but
//! indentation, trailing whitespace and blank lines, which is what the previous formatter did and is
//! now [`reindent`], kept as the safety net: if the engine's output ever fails the check that it
//! contains the same tokens as its input, the result is the re-indented source instead.
//!
//! * [`style`] — the preferences and how the layers (global, detected, `.editorconfig`, project)
//!   combine.
//! * [`imports`] — sorting, grouping and wildcard collapsing of the import block.
//! * [`editorconfig`] — the subset of `.editorconfig` that maps to a style key.
//! * [`detect`] — measuring a project's existing code for the style it already follows.

pub mod detect;
pub mod editorconfig;
mod engine;
pub mod imports;
mod classify;
mod comments;
mod layout;
mod lex;
mod render;
mod spacing;
mod structure;
mod wrap;
pub mod reindent;
pub mod style;

pub use engine::{format_edits, format_range, format_source};
pub use style::{AnnotationPlacement, BraceStyle, FormatStyle, LineEnding, StaticImports, WrapPolicy};
