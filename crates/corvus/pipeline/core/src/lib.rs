//! `corvus-pipeline-core` — the host-free run-tracking core of Corvus
//! pipelines.
//!
//! Builds on [`corvus_pipeline_api`] (the model + expression engine) and
//! holds the pieces of the orchestrator that have no Tauri / threading /
//! process coupling:
//!
//! - [`registry`] — the in-memory [`registry::PipelineRegistry`]: definitions,
//!   runs, concurrency locks, cancel tokens, running-count bookkeeping.
//! - [`persist`] — JSON run persistence under `~/.config/arbor/pipeline_runs/`
//!   and registry recovery at boot.
//! - [`run_tree`] — pure orchestration helpers: step-tree lookup, resume-cursor
//!   computation, the resumable-step index plan, output chunk splitting, and
//!   log-level inference.
//!
//! The live orchestrator (the per-run thread, `AppHandle` event emission,
//! shell-process spawning, Lua-op dispatch) stays in the host shell and
//! consumes this crate.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `corvus_pipeline_core::prelude::...`.

pub mod persist;
pub mod prelude;
pub mod registry;
pub mod run_tree;
