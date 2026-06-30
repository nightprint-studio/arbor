//! `corvus-pipeline-api` — the host-free pipeline model + expression engine.
//!
//! The pure core of Corvus pipelines (no Tauri / no live orchestrator):
//!
//! - [`vars`] — the per-run typed variable store ([`vars::RunContext`] /
//!   [`vars::VarValue`]), `${var}` string interpolation, and the declarative
//!   capture transform chain ([`vars::Transform`]).
//! - [`condition`] — the structured if-block condition tree
//!   ([`condition::Condition`]) and its [`condition::evaluate`].
//! - [`condition_parser`] — the recursive-descent parser for the free-form
//!   condition syntax (`${has_pom} && !${skip}`, `defined(x)`, …).
//! - [`builtin`] — the small side-effecting op set (file/env/JSON inspection)
//!   the runtime resolves directly to feed `${var}` captures.
//! - [`if_block`] — the `if`/`elif`/`else` branch structure whose bodies are
//!   [`model::StepDef`]s, plus branch selection.
//! - [`model`] — the step / stage / pipeline definitions and the run-state
//!   snapshots (status machine, per-step/-stage runs, log buffer entry,
//!   resume cursor) the orchestrator streams to the UI.
//!
//! The live orchestrator (threads, process spawning, event emission) stays
//! host-side; the registry that holds runs lives in `corvus-pipeline-core`.
//! Everything here is `serde`/`regex`-only and trivially testable.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `corvus_pipeline_api::prelude::...`.

pub mod builtin;
pub mod condition;
pub mod condition_parser;
pub mod if_block;
pub mod model;
pub mod prelude;
pub mod vars;
