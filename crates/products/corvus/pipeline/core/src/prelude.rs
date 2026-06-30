//! Canonical entry point for `corvus-pipeline-core`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_pipeline_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation but are not the canonical call-site path.

pub use crate::persist::{
    load_persisted_runs, now_ms, persist_run, registry_from_disk, remove_persisted_run,
    RUN_LOG_CAP,
};
pub use crate::registry::PipelineRegistry;
pub use crate::run_tree::{
    compute_resume_cursor, drain_partial_line, find_step_mut, infer_step_log_level,
    resumable_step_indices, split_chunk_lines, step_preview,
};
