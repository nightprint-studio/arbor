//! `corvus-pipeline-api` — the pure pipeline expression engine.
//!
//! The host-free core of Corvus pipelines:
//!
//! - [`vars`] — the per-run typed variable store ([`vars::RunContext`] /
//!   [`vars::VarValue`]), `${var}` string interpolation, and the declarative
//!   capture transform chain ([`vars::Transform`]).
//! - [`condition`] — the structured if-block condition tree
//!   ([`condition::Condition`]) and its [`condition::evaluate`].
//! - [`condition_parser`] — the recursive-descent parser for the free-form
//!   condition syntax (`${has_pom} && !${skip}`, `defined(x)`, …).
//!
//! The orchestrator and the `IfBlock` (which carries `StepDef` bodies) stay in
//! the host pipeline module; this crate is just the evaluation primitives, so
//! it's `serde`/`regex`-only and trivially testable. Extracted in round-2 M2;
//! when `pipeline-core` lands, the step DTOs + trait join this `*-api` leaf.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `corvus_pipeline_api::prelude::...`.

pub mod condition;
pub mod condition_parser;
pub mod prelude;
pub mod vars;
