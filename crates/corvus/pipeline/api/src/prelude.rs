//! Canonical entry point for `corvus-pipeline-api`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_pipeline_api::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation but are not the canonical call-site path.

pub use crate::builtin::{describe, run_builtin, BuiltinOutcome, BuiltinSpec};
pub use crate::condition::{evaluate, CompareOp, Condition};
pub use crate::condition_parser::parse;
pub use crate::if_block::{BranchSelection, IfBlock, IfBranch};
pub use crate::model::{
    parse_log_level, parse_stage_mode, LogEvent, LogLevel, LuaOpSpec, PipelineDef, PipelineRun,
    ResumeCursor, RunStatus, StageDef, StageMode, StageRun, StepDef, StepRun,
};
pub use crate::vars::{
    apply_transforms, resolve_vars, resolve_vars_in_json, CaptureSource, CaptureSpec, RunContext,
    Transform, VarValue,
};
