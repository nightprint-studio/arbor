//! Canonical entry point for `merula-lang`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public surface
//! through a `prelude` module. Consumers reach it via
//! `merula_lang::prelude::...` (or `use ...prelude::*;` once per file)
//! rather than the per-feature submodule paths. The submodules stay `pub` for
//! rustdoc navigation only.

// ── AST ──────────────────────────────────────────────────────────────────────
pub use crate::ast::{
    BinOp, Expr, ExprKind, FnDef, Ident, Import, Island, IslandKind, Item, Leaf, LetBind, Mini,
    MiniArg, MiniKind, Postfix, Program, UnOp,
};

// ── Errors ───────────────────────────────────────────────────────────────────
pub use crate::error::{LangError, LangErrorKind, Result};

// ── Evaluation ────────────────────────────────────────────────────────────────
pub use crate::config::EvalConfig;
pub use crate::env::Env;
pub use crate::eval::{evaluate, Ctx};
pub use crate::inject::{LogLevel, LogSink, NoImports, SilentLog, SourceLoader};
pub use crate::parse::parse;
pub use crate::value::{EvalOutput, Func, Scene, Transform, Value};

// ── Emit / materialise (AST → source, haps → mini-notation) ──────────────────
pub use crate::emit::{emit, emit_expr};
pub use crate::materialize::{materialize_island, materialize_source};

// ── Chord vocabulary (the `'name` postfix table — single source of truth) ─────
pub use crate::eval::chords::chord_intervals;

// ── DSL reference catalogue (canonical name/doc surface for tooling) ──────────
pub use crate::reference::{
    combinator_names, generator_names, log_names, reference, signal_names,
    transform_names, DslEntry, DslKind, DslParam,
};

// ── Re-exported pattern surface (so consumers need one `use`) ─────────────────
pub use merula_pattern::prelude::{ControlMap, Pattern, SourceSpan, TempoMap, TimeSpan};
