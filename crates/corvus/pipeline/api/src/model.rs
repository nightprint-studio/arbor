//! Pipeline data model — the static definitions plugins register and the
//! dynamic run state the orchestrator mutates.
//!
//! Everything here is pure serde + small value logic (no host coupling): the
//! step / stage / pipeline definitions, the run-status state machine, the
//! per-run / per-stage / per-step snapshots streamed to the UI, the log
//! buffer entry, and the resume cursor. The live orchestrator (threads,
//! process spawning, event emission) lives host-side; the registry that holds
//! these runs lives in `corvus-pipeline-core`.

use serde::{Deserialize, Serialize};

use crate::builtin::BuiltinSpec;
use crate::if_block::IfBlock;
use crate::vars::CaptureSpec;

// ===========================================================================
// Enums & simple helpers
// ===========================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel { Debug, Info, Warn, Error }

impl Default for LogLevel {
    fn default() -> Self { Self::Info }
}

impl LogLevel {
    pub fn rank(self) -> u8 {
        match self { Self::Debug => 0, Self::Info => 1, Self::Warn => 2, Self::Error => 3 }
    }
    pub fn tag(self) -> &'static str {
        match self { Self::Debug => "DEBUG", Self::Info => "INFO", Self::Warn => "WARN", Self::Error => "ERROR" }
    }
}

/// Parse a log level from a case-insensitive string (e.g. user-supplied).
/// Unknown or empty values fall back to `LogLevel::default()` (Info).
pub fn parse_log_level(s: Option<&str>) -> LogLevel {
    match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
        Some("debug") => LogLevel::Debug,
        Some("info")  => LogLevel::Info,
        Some("warn") | Some("warning") => LogLevel::Warn,
        Some("error") => LogLevel::Error,
        _ => LogLevel::default(),
    }
}

/// Parse a stage execution mode from a case-insensitive string.
/// Unknown or empty values fall back to `StageMode::default()` (Sequential).
pub fn parse_stage_mode(s: Option<&str>) -> StageMode {
    match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
        Some("parallel") => StageMode::Parallel,
        Some("sequential") | Some("serial") => StageMode::Sequential,
        _ => StageMode::default(),
    }
}

/// Execution mode for the steps inside a stage.
/// - `Sequential` (default): steps run one after the other; the first failure
///   whose `allow_failure=false` halts the stage.
/// - `Parallel`: all steps of the stage are spawned concurrently, bounded by
///   `max_parallel` (None = unlimited). The stage is considered failed only
///   after ALL parallel steps have finished — late cancellation is avoided.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageMode { Sequential, Parallel }

impl Default for StageMode {
    fn default() -> Self { Self::Sequential }
}

// ===========================================================================
// Step / Stage / Pipeline definitions (static, registered by plugins)
// ===========================================================================

/// LuaOp step: invoke a plugin-registered Lua handler instead of spawning a
/// shell process. Plugins register handlers via `arbor.pipeline.register_op()`
/// and reference them from a `StepDef` by setting `lua_op`. The handler is
/// called with `params` as its single argument and returns
/// `{ exit_code?, stdout?, stderr? }` (or raises → Failed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuaOpSpec {
    /// Plugin that registered the op. Defaults to the pipeline's `plugin`
    /// field when None (the common case: a plugin calls its own ops).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Op name registered via `arbor.pipeline.register_op(name, handler)`.
    pub op: String,
    /// Arbitrary JSON passed to the handler as a single Lua table arg.
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDef {
    pub id:      String,
    pub name:    String,
    /// Shell command to execute (run through `sh -c` / `cmd /C`). Used when
    /// `lua_op` / `builtin` / `if_block` are all None. Kept as plain
    /// `String` (not `Option<String>`) to preserve backwards compatibility
    /// with persisted runs + TS types that predate LuaOp: old JSON files
    /// have `"command": "..."` at this path.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// When present, the step invokes a plugin-registered Lua op instead of
    /// spawning a shell process. Takes precedence over `command` when both
    /// fields are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lua_op: Option<LuaOpSpec>,
    /// Built-in op (file_exists, file_read, env, json_get, …). Resolved by
    /// the runtime without spawning a shell or dispatching to Lua. Used
    /// primarily to populate `${var}` values via `capture` so later steps
    /// and `if_block` conditions can branch on them. Takes precedence over
    /// `lua_op` and `command`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinSpec>,
    /// `if`/`elif`/`else` block. When present, the step is a "control
    /// step": no command/lua_op runs at this level — the orchestrator
    /// evaluates each branch's condition in order, executes the chosen
    /// branch's nested steps, and aggregates their outcomes into the
    /// parent step's `children` (in `StepRun`). Takes precedence over
    /// every other step kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_block: Option<IfBlock>,
    /// Working directory. Shell: `current_dir` of the spawned process.
    /// LuaOp / builtin: used to anchor relative paths.
    pub cwd: Option<String>,
    /// If true the stage continues even if this step exits non-zero.
    #[serde(default)]
    pub allow_failure: bool,
    /// Extra env vars overlaid on the parent process env when running a shell
    /// `command`. Ignored by `lua_op` steps (Lua handlers spawn their own
    /// processes and own their env). Order is "parent process env, then
    /// these overrides" — so callers can inject JAVA_HOME / PATH / etc.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,
    /// Optional capture spec — after the step finishes, the orchestrator
    /// extracts the chosen `source` (stdout / exit_code / return_value /
    /// …), pipes it through `transforms`, and stores the final value under
    /// `var` in the run's variable bag for use by later steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<CaptureSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDef {
    pub id:    String,
    pub name:  String,
    pub steps: Vec<StepDef>,
    #[serde(default)]
    pub mode:  StageMode,
    /// When `mode = Parallel`, cap the number of steps running at once.
    /// `None` means "no cap" — spawn all in parallel.
    #[serde(default)]
    pub max_parallel: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDef {
    pub id:          String,
    pub name:        String,
    pub plugin:      String,
    pub description: Option<String>,
    pub icon:        Option<String>,
    pub stages:      Vec<StageDef>,
    /// Concurrency key. Only one run with this key may be `Running` at any
    /// time; starting a second one without releasing the lock is rejected.
    /// When `None`, the runtime defaults to `"<plugin>:<id>"` — i.e. one run
    /// per pipeline definition.
    #[serde(default)]
    pub lock_key:    Option<String>,
    /// Minimum log level captured in the run's log buffer. Default `Info`.
    /// Events below this level are filtered out both from the buffer and
    /// from the `arbor://pipeline-log` event stream.
    #[serde(default)]
    pub log_level:   LogLevel,
    /// Suppress the host's automatic start-toast / done-notification for
    /// runs of this pipeline. Plugins that already surface their own
    /// "started" / "finished" messages set this to `true` to avoid
    /// duplication. The `arbor.pipeline.run{ silent = ... }` per-run
    /// override takes precedence over this default.
    #[serde(default)]
    pub silent:      bool,
}

impl PipelineDef {
    /// Resolve the lock key honoring the default rule.
    pub fn effective_lock_key(&self) -> String {
        self.lock_key.clone().unwrap_or_else(|| format!("{}:{}", self.plugin, self.id))
    }

    /// Build a fresh `PipelineRun` skeleton (all stages/steps Pending) seeded
    /// with the def's lock_key, log_level and the caller-provided repo_path.
    /// Used by both the Tauri command and the Lua `arbor.pipeline.run` wrapper
    /// so lock/log metadata stay in sync with the definition.
    pub fn new_run(&self, run_id: String, repo_path: Option<String>) -> PipelineRun {
        let stages_run: Vec<StageRun> = self.stages.iter().map(|s| StageRun {
            def_id: s.id.clone(),
            name:   s.name.clone(),
            status: RunStatus::Pending,
            steps:  s.steps.iter().map(|st| StepRun {
                def_id:      st.id.clone(),
                name:        st.name.clone(),
                status:      RunStatus::Pending,
                output:      Vec::new(),
                started_at:  None,
                finished_at: None,
                exit_code:   None,
                children:    Vec::new(),
                branch:      String::new(),
            }).collect(),
        }).collect();

        PipelineRun {
            id:            run_id,
            pipeline_id:   self.id.clone(),
            plugin:        self.plugin.clone(),
            name:          self.name.clone(),
            status:        RunStatus::Pending,
            started_at:    None,
            finished_at:   None,
            stages:        stages_run,
            lock_key:      self.effective_lock_key(),
            log_level:     self.log_level,
            log:           Vec::new(),
            resume_cursor: None,
            repo_path,
            silent:        self.silent,
            queued:        false,
        }
    }
}

// ===========================================================================
// Run state (dynamic, one instance per execution)
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    /// Run halted but resumable (set by future interactive gates).
    Paused,
    Success,
    /// Run ended in error. Resumable via `resume_pipeline_run` as long as
    /// the lock_key is free — picks up from the failing step + any steps
    /// that never executed because of the failure.
    Failed,
    /// Run was stopped by the user (or by app shutdown). Resumable via
    /// `resume_pipeline_run` — re-runs the cancelled step plus everything
    /// that hadn't started yet.
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRun {
    pub def_id:      String,
    pub name:        String,
    pub status:      RunStatus,
    /// Captured stdout + stderr lines (capped at 1 000 lines).
    pub output:      Vec<String>,
    pub started_at:  Option<i64>,
    pub finished_at: Option<i64>,
    pub exit_code:   Option<i32>,
    /// Nested step runs from an `if_block` step. Empty for leaf steps;
    /// populated lazily by the orchestrator as the chosen branch executes.
    /// The `def_id` of each child is `"<parent>.<child>"` (synthesized) so
    /// resume cursors and run snapshots stay unambiguous.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<StepRun>,
    /// Label of the branch that ran inside an `if_block` step (`if`,
    /// `elif #1`, `else`). Empty for leaf steps. Surfaced to the UI so the
    /// detail panel can show which branch was taken.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRun {
    pub def_id: String,
    pub name:   String,
    pub status: RunStatus,
    pub steps:  Vec<StepRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub ts:      i64,
    pub level:   LogLevel,
    /// `pipeline` | `stage:<stage_id>` | `step:<stage_id>.<step_id>`
    pub scope:   String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeCursor {
    /// First stage that still has work to do.
    pub stage_idx: usize,
    /// Step IDs inside `stage_idx` that must be (re-)run. Steps with IDs
    /// *not* in this list keep their previous status (typically `Success`).
    pub step_ids:  Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id:          String,
    pub pipeline_id: String,
    pub plugin:      String,
    pub name:        String,
    pub status:      RunStatus,
    pub started_at:  Option<i64>,
    pub finished_at: Option<i64>,
    pub stages:      Vec<StageRun>,
    /// Concurrency key this run will attempt to hold while `Running`.
    /// Populated at `run` time (copy of `PipelineDef::effective_lock_key`).
    /// Released when the run transitions to a terminal state.
    #[serde(default)]
    pub lock_key:    String,
    #[serde(default)]
    pub log_level:   LogLevel,
    #[serde(default)]
    pub log:         Vec<LogEvent>,
    /// When `Some`, the run is resumable — the orchestrator will skip stages
    /// and steps already marked `Success` and only execute the ones listed.
    /// Set on terminal `Failed` state; cleared when a resume starts.
    #[serde(default)]
    pub resume_cursor: Option<ResumeCursor>,
    /// Working directory the run was started with. Persisted so that a resume
    /// (possibly after app restart) keeps executing steps against the same
    /// repo even if the active tab has changed.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// When `true`, the frontend skips the automatic start-toast and
    /// done-notification for this run. Inherited from `PipelineDef::silent`
    /// at run construction; can be overridden per run via Lua's
    /// `arbor.pipeline.run{ silent = ... }`.
    #[serde(default)]
    pub silent: bool,
    /// `true` while the orchestrator thread is parked waiting for a
    /// concurrency slot (the global cap from
    /// `config.pipelines.max_concurrent_runs` is full). Only meaningful
    /// when `status == Pending` — flips back to `false` the instant the
    /// run transitions to `Running`. Drives the "queued" badge in the
    /// Pipelines panel so the user can tell a parked run from one that
    /// is just about to start.
    #[serde(default)]
    pub queued: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log_level_is_case_insensitive_with_fallback() {
        assert_eq!(parse_log_level(Some("DEBUG")), LogLevel::Debug);
        assert_eq!(parse_log_level(Some(" Warn ")), LogLevel::Warn);
        assert_eq!(parse_log_level(Some("warning")), LogLevel::Warn);
        assert_eq!(parse_log_level(Some("nonsense")), LogLevel::Info);
        assert_eq!(parse_log_level(None), LogLevel::Info);
    }

    #[test]
    fn parse_stage_mode_accepts_serial_alias() {
        assert_eq!(parse_stage_mode(Some("parallel")), StageMode::Parallel);
        assert_eq!(parse_stage_mode(Some("serial")), StageMode::Sequential);
        assert_eq!(parse_stage_mode(Some("sequential")), StageMode::Sequential);
        assert_eq!(parse_stage_mode(None), StageMode::Sequential);
    }

    #[test]
    fn log_level_rank_is_ordered() {
        assert!(LogLevel::Debug.rank() < LogLevel::Info.rank());
        assert!(LogLevel::Info.rank() < LogLevel::Warn.rank());
        assert!(LogLevel::Warn.rank() < LogLevel::Error.rank());
    }

    fn sample_def() -> PipelineDef {
        PipelineDef {
            id: "build".into(),
            name: "Build".into(),
            plugin: "demo".into(),
            description: None,
            icon: None,
            stages: vec![StageDef {
                id: "s1".into(),
                name: "Stage 1".into(),
                mode: StageMode::Sequential,
                max_parallel: None,
                steps: vec![
                    StepDef {
                        id: "a".into(), name: "A".into(), command: "echo a".into(),
                        lua_op: None, builtin: None, if_block: None, cwd: None,
                        allow_failure: false, env: Default::default(), capture: None,
                    },
                    StepDef {
                        id: "b".into(), name: "B".into(), command: "echo b".into(),
                        lua_op: None, builtin: None, if_block: None, cwd: None,
                        allow_failure: false, env: Default::default(), capture: None,
                    },
                ],
            }],
            lock_key: None,
            log_level: LogLevel::Info,
            silent: false,
        }
    }

    #[test]
    fn effective_lock_key_defaults_to_plugin_colon_id() {
        let def = sample_def();
        assert_eq!(def.effective_lock_key(), "demo:build");
    }

    #[test]
    fn new_run_builds_pending_skeleton_mirroring_def_shape() {
        let def = sample_def();
        let run = def.new_run("pipe-run-1".into(), Some("/repo".into()));
        assert_eq!(run.status, RunStatus::Pending);
        assert_eq!(run.lock_key, "demo:build");
        assert_eq!(run.repo_path.as_deref(), Some("/repo"));
        assert_eq!(run.stages.len(), 1);
        assert_eq!(run.stages[0].steps.len(), 2);
        assert!(run.stages[0].steps.iter().all(|s| s.status == RunStatus::Pending));
        assert_eq!(run.stages[0].steps[0].def_id, "a");
    }
}
