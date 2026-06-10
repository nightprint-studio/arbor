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
//! ## Status (staged build)
//!
//! - [`ast`] + [`error`] — the typed tree and span-aware diagnostics (present).
//! - evaluator (`AST → Pattern`), emitter (`AST → source`), and the Tree-sitter
//!   front end (`grammar.js` + external `scanner.c` + generated `parser.c`,
//!   compiled by `build.rs` via the `cc` crate) are wired in following steps.
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention).

pub mod ast;
pub mod config;
pub(crate) mod convert;
pub mod env;
pub mod error;
pub mod eval;
pub mod inject;
pub mod parse;
pub mod prelude;
pub mod value;
