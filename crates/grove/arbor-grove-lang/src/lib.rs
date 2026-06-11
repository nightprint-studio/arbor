//! # arbor-grove-lang
//!
//! The `.grove` **language**: the bridge between source text and the pure
//! [`Pattern`](arbor_grove_pattern::prelude::Pattern) algebra of
//! `arbor-grove-pattern` (Fase 0). This is **Fase 1** — no audio, no scheduler.
//!
//! ```text
//! source (.grove)  ──parse──▶  CST  ──walk──▶  AST  ──eval──▶  Pattern
//!                                               AST  ──emit──▶  source
//! ```
//!
//! The layers are deliberately **decoupled** (`design/grove/editing-model.md`):
//! the [`ast`] is the contract, the evaluator turns it into a `Pattern`, and the
//! emitter prints it back — so the future editor can evaluate a sub-tree and
//! re-emit it (materialisation) without cross-layer dependencies.
//!
//! ## Layers
//!
//! - [`ast`] + [`error`] — the typed tree and span-aware diagnostics.
//! - [`parse`] — the Tree-sitter front end (`grammar.js` + external `scanner.c`
//!   + generated `parser.c`, compiled by `build.rs` via the `cc` crate) and the
//!   CST→AST walker: `source → Program`.
//! - [`eval`] (`AST → Pattern`), [`emit`] (`AST → source`), and
//!   [`materialize`] (evaluated haps → mini-notation AST).
//!
//! The loop closes: `parse(emit(ast))` ≈ `ast` (modulo spans), and `import`
//! resolution parses loaded modules through [`parse`].
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention).

pub mod ast;
pub mod config;
pub(crate) mod convert;
pub mod emit;
pub mod env;
pub mod error;
pub mod eval;
pub mod inject;
pub mod materialize;
pub mod parse;
pub mod prelude;
pub mod value;
