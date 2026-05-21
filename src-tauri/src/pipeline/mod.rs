// `ci_client` was relocated to `crate::git_provider::ci_impl` in Phase 5 of
// the GitProvider refactor.  This module keeps only the provider-neutral
// pipeline orchestrator.

pub mod vars;
pub mod builtin;
pub mod condition;
pub mod condition_parser;

pub use vars::{VarValue, RunContext, CaptureSpec, CaptureSource};
pub use builtin::BuiltinSpec;
pub use condition::IfBlock;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use crate::process_ext::NoWindowExt;

/// Type alias for the per-run variable context shared between every step
/// execution. Wrapped in `Arc<Mutex<…>>` so parallel stages can both read
/// and write without aliasing rules tripping us — see `vars::RunContext`
/// for the concurrency tradeoffs.
pub type RunCtx = Arc<Mutex<RunContext>>;

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
    fn rank(self) -> u8 {
        match self { Self::Debug => 0, Self::Info => 1, Self::Warn => 2, Self::Error => 3 }
    }
    fn tag(self) -> &'static str {
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

// ===========================================================================
// Registry
// ===========================================================================

#[derive(Default)]
pub struct PipelineRegistry {
    pub defs:          Vec<PipelineDef>,
    pub runs:          Vec<PipelineRun>,
    /// Cancel tokens keyed by run_id.
    pub cancel_tokens: HashMap<String, Arc<AtomicBool>>,
    /// lock_key -> run_id currently holding the lock (only set when the run
    /// is actively `Running`). `Failed` / `Paused` / `Success` / `Cancelled`
    /// runs DO NOT hold the lock — the lock is released the moment the run
    /// leaves `Running`.
    pub locks:         HashMap<String, String>,
    counter:           u64,
    /// Number of runs currently in `Running` state — bookkept by the
    /// orchestrator threads under the registry lock so `acquire_run_slot`
    /// / `release_run_slot` are race-free against the global concurrency
    /// cap. Always paired with `AppState::pipeline_cv` for queue wake-ups.
    pub running_count: usize,
}

impl PipelineRegistry {
    pub fn new_run_id(&mut self) -> String {
        self.counter += 1;
        format!("pipe-run-{}", self.counter)
    }

    /// Register (or replace) a pipeline definition.
    pub fn register_def(&mut self, def: PipelineDef) {
        if let Some(existing) = self.defs.iter_mut()
            .find(|d| d.id == def.id && d.plugin == def.plugin)
        {
            *existing = def;
        } else {
            self.defs.push(def);
        }
    }

    /// Add a new run. Keeps only the last 50 runs. Does NOT acquire the
    /// lock — the orchestrator does that as soon as it transitions to
    /// Running, so queued runs don't block the lock_key unnecessarily.
    pub fn add_run(&mut self, run: PipelineRun, cancel: Arc<AtomicBool>) {
        self.cancel_tokens.insert(run.id.clone(), cancel);
        self.runs.push(run);
        if self.runs.len() > 50 {
            let old_id = self.runs.remove(0).id;
            self.cancel_tokens.remove(&old_id);
            remove_persisted_run(&old_id);
        }
    }

    /// Overwrite an existing run with an updated snapshot.
    pub fn update_run(&mut self, run: PipelineRun) {
        if let Some(slot) = self.runs.iter_mut().find(|r| r.id == run.id) {
            *slot = run;
        }
    }

    pub fn get_run(&self, run_id: &str) -> Option<&PipelineRun> {
        self.runs.iter().find(|r| r.id == run_id)
    }

    /// Signal the orchestrator for this run to stop after the current step.
    pub fn cancel(&mut self, run_id: &str) {
        if let Some(token) = self.cancel_tokens.get(run_id) {
            token.store(true, Ordering::Relaxed);
        }
    }

    /// Try to take the lock for `lock_key` on behalf of `run_id`.
    /// Returns Err(current_owner_run_id) when another run already owns it.
    /// Idempotent when `run_id` is already the owner.
    pub fn try_acquire_lock(&mut self, lock_key: &str, run_id: &str) -> std::result::Result<(), String> {
        if let Some(owner) = self.locks.get(lock_key) {
            if owner != run_id {
                return Err(owner.clone());
            }
            return Ok(());
        }
        self.locks.insert(lock_key.to_string(), run_id.to_string());
        Ok(())
    }

    /// Release any lock owned by `run_id` (no-op when it holds none).
    pub fn release_lock_of(&mut self, run_id: &str) {
        self.locks.retain(|_, owner| owner != run_id);
    }

    /// Returns the run_id currently holding `lock_key`, if any.
    pub fn locked_by(&self, lock_key: &str) -> Option<&str> {
        self.locks.get(lock_key).map(|s| s.as_str())
    }

    /// Drop a run (and its persisted file). Only call this on a terminal run
    /// that does NOT hold any lock — callers should ensure that themselves.
    pub fn discard(&mut self, run_id: &str) {
        self.cancel_tokens.remove(run_id);
        self.runs.retain(|r| r.id != run_id);
        remove_persisted_run(run_id);
    }
}

// ===========================================================================
// Utility
// ===========================================================================

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ===========================================================================
// Persistence  (JSON per-run under ~/.config/arbor/pipeline_runs/<run_id>.json)
// ===========================================================================

const RUN_LOG_CAP: usize = 5_000;

fn run_store_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("arbor").join("pipeline_runs"))
}

fn persist_run(run: &PipelineRun) {
    let Some(dir) = run_store_dir() else { return; };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("pipeline: cannot create run store dir: {e}");
        return;
    }
    let path = dir.join(format!("{}.json", run.id));
    match serde_json::to_string_pretty(run) {
        Ok(s) => if let Err(e) = std::fs::write(&path, s) {
            tracing::warn!("pipeline: cannot persist run {}: {e}", run.id);
        },
        Err(e) => tracing::warn!("pipeline: cannot serialize run {}: {e}", run.id),
    }
}

fn remove_persisted_run(run_id: &str) {
    if let Some(dir) = run_store_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{run_id}.json")));
    }
}

/// Build a `PipelineRegistry` pre-populated with runs restored from disk.
/// The internal `counter` is advanced past the highest recovered run id so
/// new runs don't collide with persisted files.
pub fn registry_from_disk() -> PipelineRegistry {
    let runs = load_persisted_runs();
    let max_id = runs.iter()
        .filter_map(|r| r.id.strip_prefix("pipe-run-"))
        .filter_map(|n| n.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let mut reg = PipelineRegistry::default();
    reg.runs    = runs;
    reg.counter = max_id;
    reg
}

/// Load previously persisted runs from disk. Runs that were still `Running`
/// or `Pending` at shutdown are coerced to `Failed` — they cannot be safely
/// resumed because their orchestrator thread died with the process.
pub fn load_persisted_runs() -> Vec<PipelineRun> {
    let Some(dir) = run_store_dir() else { return Vec::new(); };
    let Ok(iter) = std::fs::read_dir(&dir) else { return Vec::new(); };
    let mut out = Vec::new();
    for entry in iter.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let Ok(content) = std::fs::read_to_string(&path) else { continue; };
        let Ok(mut run) = serde_json::from_str::<PipelineRun>(&content) else { continue; };
        // A run marked Running/Pending at load-time lost its orchestrator —
        // mark it Failed with a sentinel log entry so the user sees why.
        // Also drops `queued` so a parked-at-shutdown run doesn't show a
        // "Queued" badge after recovery.
        if matches!(run.status, RunStatus::Running | RunStatus::Pending) {
            run.status = RunStatus::Failed;
            run.queued = false;
            if run.finished_at.is_none() { run.finished_at = Some(now_ms()); }
            run.log.push(LogEvent {
                ts:      now_ms(),
                level:   LogLevel::Warn,
                scope:   "pipeline".into(),
                message: "run state was recovered after app restart; marked as Failed"
                    .into(),
            });
            persist_run(&run);
        }
        out.push(run);
    }
    out
}

// ===========================================================================
// Orchestrator — one background thread per pipeline run
// ===========================================================================

pub fn start_pipeline_run(
    def:        PipelineDef,
    run_id:     String,
    repo_path:  Option<String>,
    cancel:     Arc<AtomicBool>,
    app_handle: tauri::AppHandle,
) {
    if let Err(e) = std::thread::Builder::new()
        .name(format!("arbor-pipe-{run_id}"))
        .spawn(move || orchestrate(def, run_id, repo_path, cancel, app_handle))
    {
        tracing::error!("failed to spawn pipeline orchestrator thread: {e}");
    }
}

fn emit(app_handle: &tauri::AppHandle, run: &PipelineRun) {
    let _ = app_handle.emit("arbor://pipeline-update", run);
}

fn snapshot(pipelines: &Mutex<PipelineRegistry>, run_id: &str) -> Option<PipelineRun> {
    pipelines.lock().ok().and_then(|r| r.get_run(run_id).cloned())
}

/// Push a log event on the given run (filtered by its `log_level`, capped at
/// RUN_LOG_CAP entries) and broadcast it to the frontend for live streaming.
fn log_event(
    state:      &crate::AppState,
    app_handle: &tauri::AppHandle,
    run_id:     &str,
    level:      LogLevel,
    scope:      impl Into<String>,
    message:    impl Into<String>,
) {
    let scope_s = scope.into();
    let msg_s   = message.into();
    let ts      = now_ms();

    // Mutate the run's log buffer (filtered by its configured min level).
    let should_emit = {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        let Some(run)   = reg.runs.iter_mut().find(|r| r.id == run_id) else { return; };
        if level.rank() < run.log_level.rank() { return; }
        run.log.push(LogEvent { ts, level, scope: scope_s.clone(), message: msg_s.clone() });
        if run.log.len() > RUN_LOG_CAP {
            let drop = run.log.len() - RUN_LOG_CAP;
            run.log.drain(0..drop);
        }
        true
    };

    if should_emit {
        let _ = app_handle.emit("arbor://pipeline-log", serde_json::json!({
            "run_id":  run_id,
            "ts":      ts,
            "level":   level.tag(),
            "scope":   scope_s,
            "message": msg_s,
        }));
    }
}

fn fire_hook(state: &crate::AppState, hook: &str, ctx: &serde_json::Value) {
    if let Ok(host) = state.plugin_host.lock() {
        let _ = host.fire_hook(hook, &ctx.to_string());
    }
}

/// Index of steps within `stage` that actually need to run, given the run's
/// optional resume cursor. Returns `None` when the entire stage should be
/// skipped (only possible when the cursor points beyond this stage).
fn resumable_step_indices(
    stage: &StageDef,
    stage_idx: usize,
    cursor: &Option<ResumeCursor>,
) -> Option<Vec<usize>> {
    match cursor {
        None => Some((0..stage.steps.len()).collect()),
        Some(c) if c.stage_idx > stage_idx => None, // earlier stage, already succeeded
        Some(c) if c.stage_idx < stage_idx => Some((0..stage.steps.len()).collect()),
        Some(c) => {
            // Same stage as the cursor: execute only the listed step_ids.
            let wanted: std::collections::HashSet<&str> =
                c.step_ids.iter().map(|s| s.as_str()).collect();
            Some(stage.steps.iter().enumerate()
                .filter_map(|(i, s)| if wanted.contains(s.id.as_str()) { Some(i) } else { None })
                .collect())
        }
    }
}

/// Execution result of a single step — used internally to merge parallel
/// outcomes before persisting them on the run.
///
/// Children of an `if_block` step are NOT carried here: they're written
/// directly into the run state's tree as they execute (so live streaming
/// hits them via `find_step_mut`), and the merge loop leaves the parent's
/// existing `children` Vec untouched.
struct StepOutcome {
    step_idx:   usize,
    status:     RunStatus,
    exit_code:  Option<i32>,
    output:     Vec<String>,
    started_at: i64,
    finished_at: i64,
    /// `if` / `elif #N` / `else` — populated only for `if_block` steps.
    branch:     String,
}

fn execute_step(
    step_def:   &StepDef,
    cwd:        &str,
    cancel:     &Arc<AtomicBool>,
    step_idx:   usize,
    app_handle: &tauri::AppHandle,
    default_plugin: &str,
    pipeline_name:  &str,
    run_id:         &str,
    stage_id:       &str,
    ctx:            &RunCtx,
    parent_path:    &str,
) -> StepOutcome {
    let started = now_ms();
    // Effective id for log scoping + StepRun lookup. Top-level steps use
    // their plain `def.id`; nested children carry a slash-separated path
    // (`<parent>/<child>`) so siblings under different `if_block` parents
    // never collide in `find_step_mut`.
    let effective_id: String = if parent_path.is_empty() {
        step_def.id.clone()
    } else {
        format!("{parent_path}{}", step_def.id)
    };
    let sink = StepLogSink::new(
        app_handle, default_plugin, pipeline_name, run_id,
        stage_id, &effective_id, &step_def.name,
    );
    if cancel.load(Ordering::Relaxed) {
        let line = "[cancelled before start]";
        sink.emit(line);
        return StepOutcome {
            step_idx,
            status:       RunStatus::Cancelled,
            exit_code:    None,
            output:       vec![line.into()],
            started_at:   started,
            finished_at:  now_ms(),
            branch:       String::new(),
        };
    }
    // Resolve ${var} on the cwd override (step-level cwd may reference vars
    // captured by an earlier step). The caller already picked the effective
    // dir so we only run substitution if it looks templated.
    let cwd_resolved = if cwd.contains('$') {
        let c = ctx.lock().ok();
        match c {
            Some(g) => vars::resolve_vars(cwd, &g),
            None    => cwd.to_string(),
        }
    } else {
        cwd.to_string()
    };

    // Dispatch order: if_block > builtin > lua_op > shell command.
    let (exit_code, output, return_value, branch) =
        if let Some(block) = &step_def.if_block {
            execute_if_block(
                block, step_def, &cwd_resolved, cancel, app_handle,
                default_plugin, pipeline_name, run_id, stage_id, ctx,
                &effective_id, &sink,
            )
        } else if let Some(spec) = &step_def.builtin {
            let resolved_spec = match resolve_builtin_spec(spec, ctx) {
                Ok(v)  => v,
                Err(e) => {
                    let msg = format!("⚠ builtin: spec resolve failed: {e}");
                    sink.emit(&msg);
                    return StepOutcome {
                        step_idx,
                        status:       RunStatus::Failed,
                        exit_code:    Some(1),
                        output:       vec![msg],
                        started_at:   started,
                        finished_at: now_ms(),
                        branch:       String::new(),
                    };
                }
            };
            let outcome = {
                let g = ctx.lock().expect("run-ctx mutex");
                builtin::run_builtin(&resolved_spec, &cwd_resolved, &g)
            };
            for line in &outcome.lines { sink.emit(line); }
            (
                Some(outcome.exit_code),
                outcome.lines,
                Some(outcome.value),
                String::new(),
            )
        } else if let Some(op) = &step_def.lua_op {
            let resolved_params = {
                let g = ctx.lock().expect("run-ctx mutex");
                vars::resolve_vars_in_json(&op.params, &g)
            };
            let resolved_op = LuaOpSpec {
                plugin: op.plugin.clone(),
                op:     op.op.clone(),
                params: resolved_params,
            };
            let (exit, lines) = run_lua_op(
                &resolved_op, &cwd_resolved, app_handle, default_plugin, &sink,
            );
            let joined = lines.iter().filter(|l| !l.starts_with("[stderr]"))
                .cloned().collect::<Vec<_>>().join("\n");
            (exit, lines, Some(VarValue::String(joined)), String::new())
        } else {
            let (cmd, env) = {
                let g = ctx.lock().expect("run-ctx mutex");
                let c = vars::resolve_vars(&step_def.command, &g);
                let e: HashMap<String, String> = step_def.env.iter()
                    .map(|(k, v)| (k.clone(), vars::resolve_vars(v, &g)))
                    .collect();
                (c, e)
            };
            let (exit, lines) = run_command(&cmd, &cwd_resolved, &env, cancel, &sink);
            let joined = lines.iter().filter(|l| !l.starts_with("[stderr]"))
                .cloned().collect::<Vec<_>>().join("\n");
            (exit, lines, Some(VarValue::String(joined)), String::new())
        };

    // A cancel that landed mid-step makes us treat the run as Cancelled even
    // if the killed process happened to flush a 0 exit code first — without
    // this an `mvn clean package` killed by taskkill on Windows would
    // sometimes still surface as Failed/Success depending on the moment the
    // tree died, hiding the user's intent.
    //
    // For if_block steps, the "fail" semantics flow up from any failed child
    // (already encoded in the aggregated exit_code).
    let status = if cancel.load(Ordering::Relaxed) {
        RunStatus::Cancelled
    } else {
        match exit_code {
            Some(0) => RunStatus::Success,
            Some(_) | None => RunStatus::Failed,
        }
    };

    // Apply capture spec (if any). Failures inside the chain are non-fatal —
    // we set the var to Null and log the trace so the user can debug, but
    // don't promote the step to Failed because of it.
    if let Some(spec) = &step_def.capture {
        apply_capture(spec, &output, exit_code, return_value.as_ref(), ctx, &sink);
    }

    StepOutcome {
        step_idx,
        status,
        exit_code,
        output,
        started_at:  started,
        finished_at: now_ms(),
        branch,
    }
}

/// Resolve `${var}` substitutions inside a `BuiltinSpec` via a round-trip
/// through JSON. This keeps the dispatch agnostic of which fields are
/// strings — any future variant gains substitution for free.
fn resolve_builtin_spec(spec: &BuiltinSpec, ctx: &RunCtx)
    -> std::result::Result<BuiltinSpec, String>
{
    let json = serde_json::to_value(spec).map_err(|e| e.to_string())?;
    let resolved = {
        let g = ctx.lock().map_err(|_| "ctx mutex poisoned".to_string())?;
        vars::resolve_vars_in_json(&json, &g)
    };
    serde_json::from_value(resolved).map_err(|e| e.to_string())
}

/// Run an `if_block` step: pick the first matching branch, then sequentially
/// execute every nested step. Children are written directly into the parent
/// `StepRun.children` Vec (in run state) as they execute — that way live
/// streaming through `StepLogSink::emit_batch` lands on the right child via
/// the recursive `find_step_mut` lookup, and the UI can render in-flight
/// nested progress without waiting for the if_block to finish.
///
/// Returns the same four-tuple the leaf dispatch uses, so the caller can
/// merge it uniformly into the parent's `StepOutcome`.
#[allow(clippy::too_many_arguments)]
fn execute_if_block(
    block:          &IfBlock,
    parent:         &StepDef,
    cwd:            &str,
    cancel:         &Arc<AtomicBool>,
    app_handle:     &tauri::AppHandle,
    default_plugin: &str,
    pipeline_name:  &str,
    run_id:         &str,
    stage_id:       &str,
    ctx:            &RunCtx,
    parent_id:      &str,   // effective id of the parent (no trailing `/`)
    sink:           &StepLogSink,
) -> (Option<i32>, Vec<String>, Option<VarValue>, String) {
    let (selection, steps) = {
        let g = ctx.lock().expect("run-ctx mutex");
        let (sel, sts) = block.select(&g);
        (sel, sts.to_vec())
    };
    let label = selection.label();
    let mut log = vec![format!("[if] selected branch: {}", label)];
    sink.emit(&log[0]);

    let mut overall = RunStatus::Success;
    let state = app_handle.state::<crate::AppState>();

    for (i, child) in steps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            overall = RunStatus::Cancelled;
            break;
        }
        let child_def_id = format!("{parent_id}/{}", child.id);

        // Pre-create the child StepRun (status=Running) so live stdout
        // streaming finds it via `find_step_mut` while the step is in
        // flight.
        push_child_step_running(
            &state, app_handle, run_id, stage_id, parent_id,
            &child_def_id, &child.name,
        );

        let child_outcome = execute_step(
            child, cwd, cancel, i, app_handle,
            default_plugin, pipeline_name, run_id, stage_id, ctx,
            // Prefix used by execute_step to build effective_id for the
            // grandchild dispatch. Trailing `/` is intentional.
            &format!("{parent_id}/"),
        );

        finalize_child_step(
            &state, app_handle, run_id, stage_id, &child_def_id, &child_outcome,
        );

        let line = format!(
            "[if/{label}] step '{}' → {:?} (exit={:?})",
            child.name, child_outcome.status, child_outcome.exit_code,
        );
        log.push(line.clone());
        sink.emit(&line);

        let allow = child.allow_failure;
        let broke = child_outcome.status == RunStatus::Failed && !allow;
        if child_outcome.status == RunStatus::Cancelled {
            overall = RunStatus::Cancelled;
            break;
        }
        if broke { overall = RunStatus::Failed; break; }
    }

    let exit = match overall {
        RunStatus::Success => Some(0),
        RunStatus::Failed | RunStatus::Cancelled => Some(1),
        _ => None,
    };
    let return_value = Some(VarValue::String(label.clone()));
    let _ = parent; // currently unused; kept for future per-parent log scoping
    (exit, log, return_value, label)
}

/// Locate a `StepRun` anywhere inside a stage's step tree (top-level or
/// nested under any `if_block`). Used by `set_step_running`,
/// `emit_step_done`, and the live-streaming `StepLogSink` so they all work
/// uniformly regardless of nesting depth.
fn find_step_mut<'a>(steps: &'a mut [StepRun], target_id: &str) -> Option<&'a mut StepRun> {
    for s in steps.iter_mut() {
        if s.def_id == target_id { return Some(s); }
        if let Some(found) = find_step_mut(&mut s.children, target_id) {
            return Some(found);
        }
    }
    None
}

/// Push a fresh `StepRun(Running)` onto a parent's `children` Vec. Idempotent
/// against re-entry (resume / nested if-block re-evaluation): if a child
/// with that `def_id` already exists in the tree it's reset in place.
fn push_child_step_running(
    state:        &crate::AppState,
    app_handle:   &tauri::AppHandle,
    run_id:       &str,
    stage_id:     &str,
    parent_id:    &str,
    child_def_id: &str,
    child_name:   &str,
) {
    {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_id) {
                if let Some(parent) = find_step_mut(&mut s.steps, parent_id) {
                    // Reset existing or push new.
                    if let Some(existing) = parent.children.iter_mut()
                        .find(|c| c.def_id == child_def_id)
                    {
                        existing.status      = RunStatus::Running;
                        existing.output.clear();
                        existing.exit_code   = None;
                        existing.started_at  = Some(now_ms());
                        existing.finished_at = None;
                        existing.children.clear();
                        existing.branch.clear();
                    } else {
                        parent.children.push(StepRun {
                            def_id:      child_def_id.to_string(),
                            name:        child_name.to_string(),
                            status:      RunStatus::Running,
                            output:      Vec::new(),
                            started_at:  Some(now_ms()),
                            finished_at: None,
                            exit_code:   None,
                            children:    Vec::new(),
                            branch:      String::new(),
                        });
                    }
                }
            }
        }
    }
    if let Some(snap) = snapshot(&state.pipelines, run_id) {
        emit(app_handle, &snap);
    }
}

/// Write the final outcome of a child step (status, exit_code, output,
/// timing, branch label) into its already-pushed `StepRun`.
fn finalize_child_step(
    state:        &crate::AppState,
    app_handle:   &tauri::AppHandle,
    run_id:       &str,
    stage_id:     &str,
    child_def_id: &str,
    outcome:      &StepOutcome,
) {
    {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_id) {
                if let Some(child) = find_step_mut(&mut s.steps, child_def_id) {
                    child.status      = outcome.status.clone();
                    child.exit_code   = outcome.exit_code;
                    child.started_at  = Some(outcome.started_at);
                    child.finished_at = Some(outcome.finished_at);
                    child.output      = outcome.output.clone();
                    child.branch      = outcome.branch.clone();
                }
            }
        }
    }
    if let Some(snap) = snapshot(&state.pipelines, run_id) {
        emit(app_handle, &snap);
    }
}

/// Apply a `CaptureSpec` after a step completes. Writes the resulting
/// `VarValue` under `spec.var` in `ctx`. Failures are reported through
/// `sink` (debug log) but never promoted to step failure — capture is a
/// best-effort side channel.
fn apply_capture(
    spec:         &CaptureSpec,
    output_lines: &[String],
    exit_code:    Option<i32>,
    return_value: Option<&VarValue>,
    ctx:          &RunCtx,
    sink:         &StepLogSink,
) {
    let src = match spec.source {
        CaptureSource::Stdout => {
            let stdout: Vec<&String> = output_lines.iter()
                .filter(|l| !l.starts_with("[stderr]"))
                .collect();
            VarValue::String(stdout.iter().map(|s| s.as_str())
                .collect::<Vec<_>>().join("\n"))
        }
        CaptureSource::Stderr => {
            let stderr: Vec<String> = output_lines.iter()
                .filter_map(|l| l.strip_prefix("[stderr] ").map(str::to_string))
                .collect();
            VarValue::String(stderr.join("\n"))
        }
        CaptureSource::ExitCode    => match exit_code {
            Some(c) => VarValue::Number(c as f64),
            None    => VarValue::Null,
        },
        CaptureSource::Success     => VarValue::Bool(matches!(exit_code, Some(0))),
        CaptureSource::ReturnValue => return_value.cloned().unwrap_or(VarValue::Null),
    };
    let (final_value, trace) = vars::apply_transforms(src, &spec.transforms);
    {
        if let Ok(mut g) = ctx.lock() {
            g.set(spec.var.clone(), final_value.clone());
        }
    }
    let preview = final_value.as_string();
    let preview = if preview.len() > 80 {
        format!("{}…", &preview[..80])
    } else {
        preview
    };
    sink.emit(&format!("[capture] ${} = {:?}", spec.var, preview));
    for t in &trace { sink.emit(t); }
}

/// Heuristic level for a captured step-output line. Conservative — anything
/// we don't recognise stays at info, since that's the safe default for
/// arbitrary shell stdout. Mirrored on the frontend in
/// `src/lib/utils/log-highlight.ts::inferLogLevel` — keep both in sync.
///
/// `[stderr]` is NOT treated as an error signal: git/cargo/npm and most CLI
/// tools write progress and informational output to stderr by convention
/// ("Cloning into …", "Compiling foo v0.1", "Receiving objects: 42%"), so
/// flagging every stderr line as error floods the global log panel with
/// false positives. We strip the prefix and inspect the actual message
/// instead, escalating only when the body looks like a real diagnostic.
fn infer_step_log_level(line: &str) -> &'static str {
    let trimmed = line.trim_start();
    let body = trimmed.strip_prefix("[stderr]").unwrap_or(trimmed).trim_start();
    if body.starts_with('⚠')
        || body.starts_with("FAIL")
        || body.starts_with("error")
        || body.starts_with("ERROR")
        || body.starts_with("Error")
        || body.starts_with("fatal:")
        || body.starts_with("Fatal")
        || body.starts_with("panic")
    {
        "error"
    } else if body.starts_with("WARN")
        || body.starts_with("WARNING")
        || body.starts_with("warning:")
        || body.starts_with("Warning")
    {
        "warn"
    } else if body.starts_with("DEBUG") {
        "debug"
    } else {
        "info"
    }
}

/// Live sink for a single step's captured output. Cloning is cheap (all
/// fields are owned strings + the `AppHandle` Arc) so the stderr reader
/// thread can take its own copy. Each `emit` call streams the line both
/// to the global Plugin Logs panel (via `arbor://plugin-log`) and to the
/// run's own log buffer (via `log_event` → `arbor://pipeline-log`), so
/// the UI sees output as it's produced rather than in one batch when the
/// step finishes. The caller still appends every emitted line to the
/// `StepRun.output` Vec for persistence and post-mortem replay.
#[derive(Clone)]
struct StepLogSink {
    app_handle:    tauri::AppHandle,
    plugin:        String,
    pipeline_name: String,
    run_id:        String,
    stage_id:      String,
    step_id:       String,
    step_name:     String,
    scope:         String,
}

impl StepLogSink {
    fn new(
        app_handle:    &tauri::AppHandle,
        plugin:        &str,
        pipeline_name: &str,
        run_id:        &str,
        stage_id:      &str,
        step_id:       &str,
        step_name:     &str,
    ) -> Self {
        Self {
            app_handle:    app_handle.clone(),
            plugin:        plugin.to_string(),
            pipeline_name: pipeline_name.to_string(),
            run_id:        run_id.to_string(),
            stage_id:      stage_id.to_string(),
            step_id:       step_id.to_string(),
            step_name:     step_name.to_string(),
            scope:         format!("step:{stage_id}.{step_id}"),
        }
    }

    /// Per-line side effects: Plugin Logs ring buffer + run-log event.
    /// CHEAP — local mutex on `state.plugin_logs` + short level check on
    /// `state.pipelines` (returns early when below the run's `log_level`).
    /// Does **not** push to `StepRun.output` and does **not** emit
    /// `arbor://pipeline-step-output*`; both of those are batched by
    /// [`Self::emit_batch`] which the chunk reader calls once per drained
    /// pipe read instead of once per line.
    ///
    /// `[stderr]` / `WARN` / etc. prefixes are honored via
    /// [`infer_step_log_level`].
    fn record_line(&self, line: &str) {
        let level    = infer_step_log_level(line);
        let prefixed = if self.step_name.is_empty() {
            line.to_string()
        } else {
            format!("[{}] {line}", self.step_name)
        };
        crate::plugin_logs::record_with_pipeline(
            &self.app_handle, level, &self.plugin, prefixed,
            &self.pipeline_name, &self.run_id,
        );
        let state = self.app_handle.state::<crate::AppState>();
        log_event(
            &state, &self.app_handle, &self.run_id,
            LogLevel::Debug, self.scope.clone(), line.to_string(),
        );
    }

    /// Batch flush from the chunk reader. ONE `state.pipelines` lock for
    /// the whole batch + ONE `arbor://pipeline-step-output` IPC event
    /// carrying every line read in the latest pipe drain. This is the
    /// counterpart to the integrated terminal's `read(buf) → emit`
    /// model — instead of N events for N lines, the frontend gets one
    /// event with `lines: string[]` and applies them in a single Svelte
    /// reactivity tick.
    ///
    /// No-op on empty batch.
    fn emit_batch(&self, lines: &[String]) {
        if lines.is_empty() { return; }
        let state = self.app_handle.state::<crate::AppState>();
        if let Ok(mut reg) = state.pipelines.lock() {
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == self.run_id) {
                if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == self.stage_id) {
                    // Recursive lookup: top-level step OR any nested child
                    // produced by an `if_block` parent.
                    if let Some(st) = find_step_mut(&mut s.steps, &self.step_id) {
                        st.output.extend(lines.iter().cloned());
                    }
                }
            }
        }
        let _ = self.app_handle.emit("arbor://pipeline-step-output", serde_json::json!({
            "run_id":   self.run_id,
            "stage_id": self.stage_id,
            "step_id":  self.step_id,
            "lines":    lines,
        }));
    }

    /// Single-line convenience used by `run_lua_op` and by the cancel /
    /// spawn-error paths, where the producer does not have a chunk-reader
    /// to amortise over. Internally a 1-line batch — semantically
    /// identical to the old per-line emit.
    fn emit(&self, line: &str) {
        self.record_line(line);
        self.emit_batch(&[line.to_string()]);
    }
}

/// Drain `\n`-terminated lines out of an accumulating byte buffer.
///
/// `leftover` is the per-pipe state owned by the reader thread:
///   · on entry it holds bytes received in previous chunks that did not
///     yet contain a complete line;
///   · `new_data` is appended to it;
///   · every `\n` produces a `String` line (with trailing `\r` stripped
///     to handle CRLF cleanly), which is removed from the front of
///     `leftover`;
///   · bytes after the last `\n` stay in `leftover` for the next call.
///
/// At pipe EOF the caller is expected to call [`drain_partial_line`] to
/// emit any remaining tail that never received a `\n`.
fn split_chunk_lines(leftover: &mut Vec<u8>, new_data: &[u8]) -> Vec<String> {
    leftover.extend_from_slice(new_data);
    let mut lines = Vec::new();
    let mut consumed = 0usize;
    let mut i = consumed;
    while i < leftover.len() {
        if leftover[i] == b'\n' {
            let mut end = i;
            if end > consumed && leftover[end - 1] == b'\r' { end -= 1; }
            let bytes = &leftover[consumed..end];
            lines.push(String::from_utf8_lossy(bytes).into_owned());
            consumed = i + 1;
        }
        i += 1;
    }
    if consumed > 0 { leftover.drain(0..consumed); }
    lines
}

/// Flush any trailing bytes in `leftover` as a final partial line.
/// Call once at pipe EOF — the child may have written a non-newline-
/// terminated tail (e.g. progress dot, ANSI sequence with no `\n`)
/// which would otherwise be lost.
fn drain_partial_line(leftover: &mut Vec<u8>) -> Option<String> {
    if leftover.is_empty() { return None; }
    let s = String::from_utf8_lossy(leftover).into_owned();
    leftover.clear();
    Some(s)
}

/// Build a `ResumeCursor` for a terminal-but-incomplete run by walking its
/// stages in order and picking the first one that contains any step which
/// did NOT finish in `Success`. The cursor's `step_ids` lists every such
/// step in that stage so resume re-executes:
///   · the failing step,
///   · steps that came after it in sequential mode and never ran (Pending),
///   · steps cancelled by `mark_remaining_cancelled` (Cancelled),
///   · cancelled steps from a parallel stage.
/// Steps explicitly marked `allow_failure = true` that ended in `Failed` are
/// excluded — the original run already accepted them as non-fatal.
/// Returns `None` when every step succeeded (nothing to resume).
fn compute_resume_cursor(run: &PipelineRun, def: &PipelineDef) -> Option<ResumeCursor> {
    for (si, stage) in run.stages.iter().enumerate() {
        let stage_def = def.stages.iter().find(|sd| sd.id == stage.def_id);
        let pending: Vec<String> = stage.steps.iter()
            .filter(|st| match st.status {
                RunStatus::Success => false,
                RunStatus::Failed  => !stage_def
                    .and_then(|sd| sd.steps.iter().find(|s| s.id == st.def_id))
                    .map(|s| s.allow_failure)
                    .unwrap_or(false),
                _ => true, // Pending / Running / Cancelled / Paused → re-run
            })
            .map(|st| st.def_id.clone())
            .collect();
        if !pending.is_empty() {
            return Some(ResumeCursor { stage_idx: si, step_ids: pending });
        }
    }
    None
}

/// Outcome of `acquire_run_slot`.
enum SlotAcquire {
    /// A slot was reserved (`running_count` incremented). Caller MUST pair
    /// with `release_run_slot` exactly once when the run leaves Running.
    Acquired,
    /// The cancel token fired while the orchestrator was parked waiting for
    /// a slot — no slot was reserved. Caller should mark the run Cancelled,
    /// release the lock_key and bail out.
    Cancelled,
}

/// Read the configured global cap for concurrent pipeline runs.  `0` means
/// unlimited.  Falls back to the default when the config mutex is poisoned.
fn read_max_concurrent_runs(state: &crate::AppState) -> u32 {
    state.config.lock().ok()
        .map(|c| c.pipelines.max_concurrent_runs)
        .unwrap_or(4)
}

/// Wait for a free concurrency slot, then increment `running_count`.
/// While parked the run stays `Pending` with `queued = true` — the panel
/// renders that as a "Queued" badge so the user can tell it apart from a
/// run that's about to start. `cap == 0` short-circuits to "unlimited".
///
/// Wakes on `pipeline_cv.notify_*` (terminal release) AND on a 250 ms
/// poll timeout so a freshly-raised cap or a cancel signal land within a
/// quarter second even if no other run is changing state.
fn acquire_run_slot(
    state:      &crate::AppState,
    app_handle: &tauri::AppHandle,
    run_id:     &str,
    cancel:     &Arc<AtomicBool>,
) -> SlotAcquire {
    // Fast path: read cap once, try without ever surfacing as "queued".
    let cap = read_max_concurrent_runs(state);
    {
        let Ok(mut reg) = state.pipelines.lock() else { return SlotAcquire::Cancelled; };
        if cancel.load(Ordering::Relaxed) {
            return SlotAcquire::Cancelled;
        }
        if cap == 0 || reg.running_count < cap as usize {
            reg.running_count += 1;
            return SlotAcquire::Acquired;
        }
        // Cap reached — surface the queued state to the UI before waiting.
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.queued = true;
        }
    }
    // Emit queued snapshot outside the lock so listeners can update.
    if let Some(snap) = snapshot(&state.pipelines, run_id) {
        emit(app_handle, &snap);
        persist_run(&snap);
    }
    // Wait loop: re-read cap each iteration so cap bumps via config-set
    // are picked up without restarting the run.
    let mut guard = match state.pipelines.lock() {
        Ok(g)  => g,
        Err(_) => return SlotAcquire::Cancelled,
    };
    loop {
        if cancel.load(Ordering::Relaxed) {
            if let Some(r) = guard.runs.iter_mut().find(|r| r.id == run_id) {
                r.queued = false;
            }
            return SlotAcquire::Cancelled;
        }
        let cap = read_max_concurrent_runs(state);
        if cap == 0 || guard.running_count < cap as usize {
            guard.running_count += 1;
            if let Some(r) = guard.runs.iter_mut().find(|r| r.id == run_id) {
                r.queued = false;
            }
            return SlotAcquire::Acquired;
        }
        let res = state.pipeline_cv
            .wait_timeout(guard, Duration::from_millis(250))
            .ok();
        match res {
            Some((g, _)) => guard = g,
            None         => return SlotAcquire::Cancelled,
        }
    }
}

/// Decrement the running counter and wake the next queued orchestrator.
/// Idempotent against an already-zero counter (defensive — saturating
/// arithmetic so a misuse cannot underflow into a near-`usize::MAX`
/// queue-stall state).
fn release_run_slot(state: &crate::AppState) {
    if let Ok(mut reg) = state.pipelines.lock() {
        reg.running_count = reg.running_count.saturating_sub(1);
    }
    state.pipeline_cv.notify_one();
}

fn orchestrate(
    def:        PipelineDef,
    run_id:     String,
    repo_path:  Option<String>,
    cancel:     Arc<AtomicBool>,
    app_handle: tauri::AppHandle,
) {
    let state = app_handle.state::<crate::AppState>();

    // ── Acquire the lock. Failure here aborts the run before it starts. ──
    let lock_key = def.effective_lock_key();
    {
        let Ok(mut reg) = state.pipelines.lock() else {
            tracing::error!("pipeline mutex poisoned acquiring lock for run {run_id}");
            return;
        };
        if let Err(owner) = reg.try_acquire_lock(&lock_key, &run_id) {
            // Mark this run Failed immediately with a descriptive log entry.
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                r.status      = RunStatus::Failed;
                r.started_at  = Some(now_ms());
                r.finished_at = Some(now_ms());
                r.log.push(LogEvent {
                    ts:      now_ms(),
                    level:   LogLevel::Error,
                    scope:   "pipeline".into(),
                    message: format!(
                        "cannot start: lock '{lock_key}' is already held by run '{owner}'"
                    ),
                });
            }
            let snap = reg.get_run(&run_id).cloned();
            drop(reg);
            if let Some(s) = snap {
                persist_run(&s);
                emit(&app_handle, &s);
            }
            return;
        }
    }

    // ── Wait for a global concurrency slot (config.pipelines.max_concurrent_runs).
    //    The lock_key is already held above, so a queued run still blocks
    //    other runs of the same pipeline from starting in parallel — that
    //    keeps the lock_key collision semantics exactly as documented.
    if let SlotAcquire::Cancelled = acquire_run_slot(&state, &app_handle, &run_id, &cancel) {
        // The user cancelled while we were parked. Mark Cancelled, release
        // the lock_key, snapshot + emit, and bail without ever transitioning
        // through Running. No slot was reserved → no release_run_slot call.
        {
            let Ok(mut reg) = state.pipelines.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                r.status      = RunStatus::Cancelled;
                r.queued      = false;
                r.started_at  = r.started_at.or(Some(now_ms()));
                r.finished_at = Some(now_ms());
                r.log.push(LogEvent {
                    ts:      now_ms(),
                    level:   LogLevel::Warn,
                    scope:   "pipeline".into(),
                    message: "cancelled while waiting for a concurrency slot"
                        .into(),
                });
            }
            reg.release_lock_of(&run_id);
        }
        if let Some(snap) = snapshot(&state.pipelines, &run_id) {
            persist_run(&snap);
            emit(&app_handle, &snap);
        }
        return;
    }

    // ── Mark pipeline Running + first snapshot ───────────────────────────
    {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.status     = RunStatus::Running;
            if r.started_at.is_none() { r.started_at = Some(now_ms()); }
        }
    }
    let resume_cursor_taken: Option<ResumeCursor> = {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        reg.runs.iter_mut()
            .find(|r| r.id == run_id)
            .and_then(|r| r.resume_cursor.take())
    };
    if let Some(snap) = snapshot(&state.pipelines, &run_id) {
        emit(&app_handle, &snap);
        persist_run(&snap);
        fire_hook(&state, "on_pipeline_started", &serde_json::json!({
            "run_id":      &run_id,
            "pipeline_id": &snap.pipeline_id,
            "plugin":      &snap.plugin,
        }));
        log_event(&state, &app_handle, &run_id, LogLevel::Info, "pipeline",
            if resume_cursor_taken.is_some() {
                format!("pipeline '{}' resumed", def.name)
            } else {
                format!("pipeline '{}' started", def.name)
            });
    }

    let mut overall_failed = false;

    // ── Per-run variable context ─────────────────────────────────────────
    // Empty at run start. Steps populate it via `CaptureSpec`; downstream
    // steps + `if_block` conditions read it via `${var}` interpolation.
    // Cleared on every fresh run (resume re-creates a context too — vars
    // captured by previously-Successful steps are NOT re-applied, since
    // we currently don't persist them; in practice that means a resumed
    // run re-runs only Failed/Pending steps and any vars they need must
    // come from those steps themselves).
    let ctx: RunCtx = Arc::new(Mutex::new(RunContext::new()));

    'stages: for (si, stage_def) in def.stages.iter().enumerate() {
        let step_indices = match resumable_step_indices(stage_def, si, &resume_cursor_taken) {
            Some(v) => v,
            None    => continue 'stages, // skip stages already succeeded
        };

        // Nothing to do for this stage? move on.
        if step_indices.is_empty() {
            continue 'stages;
        }

        // Cancel check (pre-stage).
        if cancel.load(Ordering::Relaxed) {
            mark_remaining_cancelled(&state.pipelines, &run_id, si);
            break 'stages;
        }

        // Mark stage Running.
        {
            let Ok(mut reg) = state.pipelines.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                if let Some(s) = r.stages.get_mut(si) { s.status = RunStatus::Running; }
            }
        }
        if let Some(snap) = snapshot(&state.pipelines, &run_id) {
            emit(&app_handle, &snap);
            persist_run(&snap);
        }
        log_event(&state, &app_handle, &run_id, LogLevel::Info,
            format!("stage:{}", stage_def.id),
            format!("stage '{}' started (mode={:?}, steps={})",
                stage_def.name, stage_def.mode, step_indices.len()));

        // ── Execute the stage's steps in the correct mode ────────────────
        let outcomes = match stage_def.mode {
            StageMode::Sequential => execute_stage_sequential(
                &def, stage_def, si, &step_indices,
                &repo_path, &cancel, &run_id, &state, &app_handle, &ctx,
            ),
            StageMode::Parallel => execute_stage_parallel(
                &def, stage_def, si, &step_indices,
                &repo_path, &cancel, &run_id, &state, &app_handle, &ctx,
            ),
        };

        // Merge outcomes into the run. `children` is INTENTIONALLY left
        // untouched — for `if_block` steps it's been mutated incrementally
        // by `execute_if_block` while the children ran, and overwriting it
        // with the (empty) outcome would erase that nested progress.
        let mut had_fatal_failure = false;
        {
            let Ok(mut reg) = state.pipelines.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                if let Some(s) = r.stages.get_mut(si) {
                    for o in &outcomes {
                        if let Some(st) = s.steps.get_mut(o.step_idx) {
                            st.status      = o.status.clone();
                            st.exit_code   = o.exit_code;
                            st.started_at  = Some(o.started_at);
                            st.finished_at = Some(o.finished_at);
                            st.output      = o.output.clone();
                            st.branch      = o.branch.clone();
                        }
                        // Honor allow_failure when deciding "fatal".
                        let allow = stage_def.steps.get(o.step_idx)
                            .map(|sd| sd.allow_failure)
                            .unwrap_or(false);
                        if o.status == RunStatus::Failed && !allow {
                            had_fatal_failure = true;
                        }
                    }
                }
            }
        }

        // Finalize stage status.
        let stage_status = if cancel.load(Ordering::Relaxed) {
            RunStatus::Cancelled
        } else if had_fatal_failure {
            RunStatus::Failed
        } else {
            RunStatus::Success
        };
        {
            let Ok(mut reg) = state.pipelines.lock() else { return; };
            if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
                if let Some(s) = r.stages.get_mut(si) { s.status = stage_status.clone(); }
            }
        }
        if let Some(snap) = snapshot(&state.pipelines, &run_id) {
            emit(&app_handle, &snap);
            persist_run(&snap);
        }
        log_event(&state, &app_handle, &run_id,
            match stage_status {
                RunStatus::Success  => LogLevel::Info,
                RunStatus::Failed   => LogLevel::Error,
                _                   => LogLevel::Warn,
            },
            format!("stage:{}", stage_def.id),
            format!("stage '{}' finished with status={:?}", stage_def.name, stage_status));

        if stage_status == RunStatus::Failed {
            overall_failed = true;
            break 'stages;
        }
    }

    // ── Finalize pipeline ────────────────────────────────────────────────
    let final_status = if cancel.load(Ordering::Relaxed) {
        RunStatus::Cancelled
    } else if overall_failed {
        RunStatus::Failed
    } else {
        RunStatus::Success
    };
    {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.status      = final_status.clone();
            r.finished_at = Some(now_ms());
            // Compute the resume cursor from the run's CURRENT step statuses
            // (after all outcome merges) for both Failed and Cancelled. This
            // captures not only the failing step but also subsequent Pending
            // steps in sequential mode + Cancelled steps from
            // `mark_remaining_cancelled`, all of which need to re-run.
            r.resume_cursor = match final_status {
                RunStatus::Failed | RunStatus::Cancelled => compute_resume_cursor(r, &def),
                _ => None,
            };
        }
        // Release the lock regardless of terminal outcome.
        reg.release_lock_of(&run_id);
    }
    // Release the concurrency slot and wake the next queued orchestrator.
    // Done OUTSIDE the registry lock above so the wake-up doesn't race
    // against `wait_timeout` re-acquiring it.
    release_run_slot(&state);

    if let Some(snap) = snapshot(&state.pipelines, &run_id) {
        emit(&app_handle, &snap);
        persist_run(&snap);
        fire_hook(&state, "on_pipeline_done", &serde_json::json!({
            "run_id":      &run_id,
            "pipeline_id": &snap.pipeline_id,
            "plugin":      &snap.plugin,
            "status":      &final_status,
        }));
        log_event(&state, &app_handle, &run_id,
            if final_status == RunStatus::Success { LogLevel::Info } else { LogLevel::Error },
            "pipeline",
            format!("pipeline '{}' finished with status={:?}", def.name, final_status));
    }
}

// ---------------------------------------------------------------------------
// Sequential / parallel stage execution
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn execute_stage_sequential(
    def:        &PipelineDef,
    stage_def:  &StageDef,
    _si:        usize,
    indices:    &[usize],
    repo_path:  &Option<String>,
    cancel:     &Arc<AtomicBool>,
    run_id:     &str,
    state:      &crate::AppState,
    app_handle: &tauri::AppHandle,
    ctx:        &RunCtx,
) -> Vec<StepOutcome> {
    let mut outcomes = Vec::with_capacity(indices.len());
    for &step_idx in indices {
        if cancel.load(Ordering::Relaxed) { break; }
        let step_def = &stage_def.steps[step_idx];
        let cwd = step_def.cwd.clone()
            .or_else(|| repo_path.clone())
            .unwrap_or_else(|| ".".to_string());

        // Mark step Running.
        set_step_running(state, run_id, stage_def.id.as_str(), step_def.id.as_str());
        if let Some(snap) = snapshot(&state.pipelines, run_id) { emit(app_handle, &snap); }
        // Log preview: prefer op name / builtin label / "if-block" when the
        // step isn't a shell command so the debug log carries something
        // meaningful.
        let preview = step_preview(step_def);
        log_event(state, app_handle, run_id, LogLevel::Info,
            format!("step:{}.{}", stage_def.id, step_def.id),
            format!("step '{}' started: {}", step_def.name, preview));
        log_event(state, app_handle, run_id, LogLevel::Debug,
            format!("step:{}.{}", stage_def.id, step_def.id),
            format!("cwd={cwd}"));

        let outcome = execute_step(
            step_def, &cwd, cancel, step_idx, app_handle,
            &def.plugin, &def.name, run_id, &stage_def.id, ctx, "",
        );
        emit_step_done(state, app_handle, run_id, stage_def, step_def, &outcome);

        let allow_failure = step_def.allow_failure;
        let broke_stage   = outcome.status == RunStatus::Failed && !allow_failure;
        outcomes.push(outcome);
        if broke_stage { break; }
    }
    outcomes
}

/// Short preview of a step's "what does it do" used in run logs and in the
/// "step started" lines. Prefers the most specific kind: if_block > builtin >
/// lua_op > shell command.
fn step_preview(step: &StepDef) -> String {
    if step.if_block.is_some() { return "if-block".to_string(); }
    if let Some(b) = &step.builtin { return builtin::describe(b); }
    if let Some(op) = &step.lua_op { return format!("lua_op {}", op.op); }
    step.command.clone()
}

#[allow(clippy::too_many_arguments)]
fn execute_stage_parallel(
    def:        &PipelineDef,
    stage_def:  &StageDef,
    _si:        usize,
    indices:    &[usize],
    repo_path:  &Option<String>,
    cancel:     &Arc<AtomicBool>,
    run_id:     &str,
    state:      &crate::AppState,
    app_handle: &tauri::AppHandle,
    ctx:        &RunCtx,
) -> Vec<StepOutcome> {
    let total = indices.len();
    let cap = stage_def.max_parallel.unwrap_or(total).max(1);
    let (tx, rx) = mpsc::channel::<StepOutcome>();
    let mut in_flight: Vec<std::thread::JoinHandle<()>> = Vec::new();
    let mut pending = indices.to_vec();
    let mut collected: Vec<StepOutcome> = Vec::with_capacity(total);

    // Mark all steps Running upfront so the UI shows them spinning.
    for &step_idx in indices {
        let step_def = &stage_def.steps[step_idx];
        set_step_running(state, run_id, stage_def.id.as_str(), step_def.id.as_str());
        let preview = step_preview(step_def);
        log_event(state, app_handle, run_id, LogLevel::Info,
            format!("step:{}.{}", stage_def.id, step_def.id),
            format!("step '{}' started (parallel): {}", step_def.name, preview));
    }
    if let Some(snap) = snapshot(&state.pipelines, run_id) { emit(app_handle, &snap); }

    let mut spawned = 0usize;
    // Spawn up to `cap` workers, refill as they finish. Drop our own sender
    // at the end of the block so lingering `rx.recv()` doesn't deadlock even
    // if a worker panics before sending.
    while collected.len() < total {
        // Fill the pool.
        while spawned - collected.len() < cap && !pending.is_empty() {
            let step_idx = pending.remove(0);
            let step_def = stage_def.steps[step_idx].clone();
            let cwd = step_def.cwd.clone()
                .or_else(|| repo_path.clone())
                .unwrap_or_else(|| ".".to_string());
            let cancel_c = cancel.clone();
            let tx_c = tx.clone();
            let app_c = app_handle.clone();
            let plugin_c   = def.plugin.clone();
            let pipeline_c = def.name.clone();
            let run_id_c   = run_id.to_string();
            let stage_id_c = stage_def.id.clone();
            let ctx_c      = ctx.clone();
            let h = std::thread::spawn(move || {
                let outcome = execute_step(
                    &step_def, &cwd, &cancel_c, step_idx, &app_c,
                    &plugin_c, &pipeline_c, &run_id_c, &stage_id_c,
                    &ctx_c, "",
                );
                let _ = tx_c.send(outcome);
            });
            in_flight.push(h);
            spawned += 1;
        }

        match rx.recv() {
            Ok(outcome) => {
                let step_def = &stage_def.steps[outcome.step_idx];
                emit_step_done(state, app_handle, run_id, stage_def, step_def, &outcome);
                collected.push(outcome);
            }
            Err(_) => break, // all senders dropped — should only happen on worker panic
        }
    }
    drop(tx);
    for h in in_flight { let _ = h.join(); }
    collected
}

fn set_step_running(
    state:    &crate::AppState,
    run_id:   &str,
    stage_id: &str,
    step_id:  &str,
) {
    let Ok(mut reg) = state.pipelines.lock() else { return; };
    if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
        if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_id) {
            if let Some(st) = find_step_mut(&mut s.steps, step_id) {
                st.status      = RunStatus::Running;
                st.started_at  = Some(now_ms());
                // Wipe previous-attempt artefacts so a resumed step doesn't
                // show stale output / exit code / finished_at / nested
                // children while the new attempt streams in.
                st.output.clear();
                st.exit_code   = None;
                st.finished_at = None;
                st.children.clear();
                st.branch.clear();
            }
        }
    }
}

fn emit_step_done(
    state:      &crate::AppState,
    app_handle: &tauri::AppHandle,
    run_id:     &str,
    stage_def:  &StageDef,
    step_def:   &StepDef,
    outcome:    &StepOutcome,
) {
    let scope = format!("step:{}.{}", stage_def.id, step_def.id);
    let level = match outcome.status {
        RunStatus::Success   => LogLevel::Info,
        RunStatus::Failed    => if step_def.allow_failure { LogLevel::Warn } else { LogLevel::Error },
        RunStatus::Cancelled => LogLevel::Warn,
        _                    => LogLevel::Info,
    };
    log_event(state, app_handle, run_id, level, scope,
        format!("step '{}' finished: {:?} (exit={:?}, elapsed={}ms)",
            step_def.name,
            outcome.status,
            outcome.exit_code,
            outcome.finished_at - outcome.started_at));

    // Captured stdout/stderr lines are NOT replayed here — they were already
    // streamed live through `StepLogSink` while the step was running, both
    // into the global Plugin Logs panel (`arbor://plugin-log`) and into the
    // run's own log (`arbor://pipeline-log` + `run.log`).

    // Push the step update into the run and snapshot+emit. Children are
    // mutated separately by `execute_if_block` while the if-block runs, so
    // we deliberately do NOT touch `st.children` here — overwriting it
    // would erase the nested progress the UI already received.
    {
        let Ok(mut reg) = state.pipelines.lock() else { return; };
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            if let Some(s) = r.stages.iter_mut().find(|s| s.def_id == stage_def.id) {
                if let Some(st) = find_step_mut(&mut s.steps, &step_def.id) {
                    st.status      = outcome.status.clone();
                    st.exit_code   = outcome.exit_code;
                    st.started_at  = Some(outcome.started_at);
                    st.finished_at = Some(outcome.finished_at);
                    st.output      = outcome.output.clone();
                    st.branch      = outcome.branch.clone();
                }
            }
        }
    }
    if let Some(snap) = snapshot(&state.pipelines, run_id) {
        emit(app_handle, &snap);
        persist_run(&snap);
        fire_hook(state, "on_pipeline_step_done", &serde_json::json!({
            "run_id":    run_id,
            "plugin":    &snap.plugin,
            "stage_id":  &stage_def.id,
            "step_id":   &step_def.id,
            "step_name": &step_def.name,
            "status":    &outcome.status,
            "exit_code": outcome.exit_code,
        }));
    }
}

fn mark_remaining_cancelled(
    pipelines: &Mutex<PipelineRegistry>,
    run_id:    &str,
    from_si:   usize,
) {
    fn cascade(steps: &mut [StepRun]) {
        for step in steps.iter_mut() {
            if matches!(step.status, RunStatus::Pending | RunStatus::Running) {
                step.status = RunStatus::Cancelled;
            }
            cascade(&mut step.children);
        }
    }
    let Ok(mut reg) = pipelines.lock() else { return; };
    if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
        for (si, stage) in r.stages.iter_mut().enumerate() {
            if si < from_si { continue; }
            stage.status = RunStatus::Cancelled;
            cascade(&mut stage.steps);
        }
    }
}

// ===========================================================================
// Resume / discard entry points (called from Tauri commands)
// ===========================================================================

/// Resume a terminal-but-incomplete run by re-executing its non-Success
/// steps (and any later stages that never ran).
/// Returns an error when:
/// - the run does not exist
/// - the run is not in a resumable state (only `Failed`, `Cancelled`, or
///   `Paused` qualify — `Success` has nothing to resume)
/// - the lock is held by a different run (another run is active)
pub fn resume_run(
    run_id:     &str,
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let state = app_handle.state::<crate::AppState>();

    // Validate + clone the data we need outside the mutex.
    let (def, repo_path) = {
        let mut reg = state.pipelines.lock()
            .map_err(|_| "pipeline mutex poisoned".to_string())?;
        let run = reg.runs.iter().find(|r| r.id == run_id)
            .ok_or_else(|| format!("run '{run_id}' not found"))?;
        match run.status {
            RunStatus::Failed | RunStatus::Paused | RunStatus::Cancelled => {},
            ref s => return Err(format!("run '{run_id}' is not resumable (status={s:?})")),
        }
        if run.resume_cursor.is_none() {
            return Err(format!("run '{run_id}' has no resume cursor"));
        }
        let def = reg.defs.iter()
            .find(|d| d.plugin == run.plugin && d.id == run.pipeline_id)
            .cloned()
            .ok_or_else(|| format!(
                "pipeline definition '{}:{}' not found (plugin unloaded?)",
                run.plugin, run.pipeline_id))?;
        // Verify the lock is free (or already ours).
        let lock_key = run.lock_key.clone();
        if let Some(owner) = reg.locked_by(&lock_key) {
            if owner != run_id {
                return Err(format!(
                    "cannot resume: lock '{lock_key}' is held by run '{owner}'"));
            }
        }
        let repo_path = run.repo_path.clone();
        // Refresh state to Pending so the orchestrator's Running transition
        // fires properly. Keep the resume_cursor intact — the orchestrator
        // will consume it.
        if let Some(r) = reg.runs.iter_mut().find(|r| r.id == run_id) {
            r.status      = RunStatus::Pending;
            r.finished_at = None;
        }
        (def, repo_path)
    };

    // Fresh cancel token.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut reg = state.pipelines.lock()
            .map_err(|_| "pipeline mutex poisoned".to_string())?;
        reg.cancel_tokens.insert(run_id.to_string(), cancel.clone());
    }

    start_pipeline_run(def, run_id.to_string(), repo_path, cancel, app_handle);
    Ok(())
}

/// Drop a failed/cancelled run — removes the on-disk file and the in-memory
/// entry. Refuses to discard a run that is currently Running.
pub fn discard_run(run_id: &str, app_handle: tauri::AppHandle) -> std::result::Result<(), String> {
    let state = app_handle.state::<crate::AppState>();
    let mut reg = state.pipelines.lock()
        .map_err(|_| "pipeline mutex poisoned".to_string())?;
    let status = reg.runs.iter().find(|r| r.id == run_id).map(|r| r.status.clone());
    match status {
        None => Err(format!("run '{run_id}' not found")),
        Some(RunStatus::Running) => Err(format!("cannot discard a Running run — cancel it first")),
        Some(_) => {
            reg.discard(run_id);
            drop(reg);
            let _ = app_handle.emit("arbor://pipeline-discarded",
                serde_json::json!({ "run_id": run_id }));
            Ok(())
        }
    }
}

// ===========================================================================
// Command execution (unchanged behavior)
// ===========================================================================

fn run_command(
    command: &str,
    cwd: &str,
    env: &std::collections::HashMap<String, String>,
    cancel: &Arc<AtomicBool>,
    sink: &StepLogSink,
) -> (Option<i32>, Vec<String>) {
    // On Windows, `Command::arg` / `args` auto-quotes arguments that contain
    // spaces or quotes — which mangles shell command strings like
    //   `git clone --progress -- "C:\path" "C:\target"`
    // because the inner quotes get doubled and cmd.exe receives them as
    // literal characters (we saw `fatal: could not create '"C:\…"'`).
    // `raw_arg` bypasses the quoting and passes bytes as-is to CreateProcessW,
    // which is exactly what we want for shell command strings.
    // stdin=null prevents the child from inheriting Arbor's stdin pipe; no
    // user-driven program sends data through there, and keeping it closed
    // means fewer handles shared across processes (smaller chance that an
    // open file in Arbor keeps a file "in use" for the child's siblings).
    #[cfg(windows)]
    let spawn_result = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        c.raw_arg("/C").raw_arg(command)
            .current_dir(cwd)
            .no_window()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in env { c.env(k, v); }
        c.spawn()
    };
    #[cfg(not(windows))]
    let spawn_result = {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", command])
            .current_dir(cwd)
            .no_window()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in env { c.env(k, v); }
        c.spawn()
    };

    let mut child = match spawn_result {
        Ok(c)  => c,
        Err(e) => {
            let msg = format!("⚠ failed to spawn: {e}");
            sink.emit(&msg);
            return (Some(1), vec![msg]);
        }
    };

    let pid = child.id();

    // Captured-output buffer shared between the stdout reader (this thread)
    // and the stderr reader (a worker thread) so both pipes' lines land in
    // a single chronologically-ordered Vec — matching the order in which
    // they were streamed live through the sink.
    let output: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Cancel watcher — without this, clicking "Stop" on a long-running step
    // (e.g. `mvn clean package`) would only take effect at the NEXT step
    // boundary, because run_command happily blocks on stdout for as long as
    // the child wants to talk. The watcher polls the cancel flag at 100ms
    // and forcibly terminates the entire process tree (`/T` on Windows,
    // SIGTERM on Unix) so Maven's spawned `java` child also dies — killing
    // just the cmd.exe wrapper would leave the build running. Exits as soon
    // as `done` is set by the main thread after `child.wait()`.
    let done = Arc::new(AtomicBool::new(false));
    let killer = {
        let cancel_c = cancel.clone();
        let done_c   = done.clone();
        let sink_c   = sink.clone();
        std::thread::spawn(move || {
            let mut announced = false;
            while !done_c.load(Ordering::Relaxed) {
                if cancel_c.load(Ordering::Relaxed) {
                    if !announced {
                        sink_c.emit("[cancel requested — terminating process tree]");
                        announced = true;
                    }
                    crate::jobs::kill_process(pid);
                    // Re-issue every tick: on Windows taskkill races with
                    // child startup; one shot can miss. Stops as soon as
                    // `done` flips after wait() returns.
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
    };

    // ── Chunk-based pipe reader ──────────────────────────────────────────
    //
    // Mirrors the integrated terminal's read loop: pull whatever bytes the
    // OS hands us in 4 KB chunks (`reader.read(buf)`), split them into
    // `\n`-terminated lines on the reader thread via [`split_chunk_lines`],
    // then call [`StepLogSink::emit_batch`] once per chunk with the lines
    // we found.  Earlier versions used `BufReader::lines() + take(500)`
    // which closed the read end of the pipe mid-build (silently dropping
    // output and forcing the child into BROKEN_PIPE handling) and emitted
    // one Tauri event per line — typically a couple of thousand events
    // for a `mvn clean package`.  The chunk model does ~5–20 events for
    // the same build and never closes the pipe early.
    //
    // `MAX_CAPTURED_LINES_PER_PIPE` caps the captured buffer so a runaway
    // step does not eat memory.  We KEEP READING the pipe past the cap —
    // just stop appending — so the child never blocks on a full pipe.
    const READ_BUF_SIZE: usize = 4096;
    const MAX_CAPTURED_LINES_PER_PIPE: usize = 5_000;

    fn drain_pipe<R: std::io::Read>(
        mut reader: R,
        sink: &StepLogSink,
        output: &Arc<Mutex<Vec<String>>>,
        stderr_prefix: bool,
    ) {
        let mut buf       = [0u8; READ_BUF_SIZE];
        let mut leftover  = Vec::<u8>::with_capacity(READ_BUF_SIZE);
        let mut captured  = 0usize;
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0)  => 0,
                Ok(n)  => n,
                Err(_) => 0,
            };
            if n == 0 {
                if let Some(tail) = drain_partial_line(&mut leftover) {
                    let line = if stderr_prefix { format!("[stderr] {tail}") } else { tail };
                    if captured < MAX_CAPTURED_LINES_PER_PIPE {
                        sink.record_line(&line);
                        if let Ok(mut v) = output.lock() { v.push(line.clone()); }
                        sink.emit_batch(std::slice::from_ref(&line));
                    }
                }
                break;
            }
            let mut chunk_lines = split_chunk_lines(&mut leftover, &buf[..n]);
            if stderr_prefix {
                for l in chunk_lines.iter_mut() {
                    *l = format!("[stderr] {l}");
                }
            }
            if chunk_lines.is_empty() { continue; }

            // Apply the per-pipe capture cap. Past the cap we keep reading
            // (to drain the pipe so the child doesn't block on WriteFile)
            // but stop emitting / appending.
            let to_emit: &[String] = if captured >= MAX_CAPTURED_LINES_PER_PIPE {
                &[]
            } else {
                let take_n = chunk_lines.len()
                    .min(MAX_CAPTURED_LINES_PER_PIPE - captured);
                captured += take_n;
                &chunk_lines[..take_n]
            };

            if !to_emit.is_empty() {
                for line in to_emit { sink.record_line(line); }
                if let Ok(mut v) = output.lock() {
                    v.extend(to_emit.iter().cloned());
                }
                sink.emit_batch(to_emit);
            }
        }
    }

    let stderr_handle = child.stderr.take().map(|se| {
        let sink_c   = sink.clone();
        let output_c = output.clone();
        std::thread::spawn(move || {
            drain_pipe(se, &sink_c, &output_c, /* stderr_prefix = */ true);
        })
    });

    if let Some(so) = child.stdout.take() {
        drain_pipe(so, sink, &output, /* stderr_prefix = */ false);
    }

    if let Some(h) = stderr_handle { let _ = h.join(); }

    let exit_code = child.wait().ok().and_then(|s| s.code());
    done.store(true, Ordering::Relaxed);
    let _ = killer.join();
    let final_output = output.lock().map(|v| v.clone()).unwrap_or_default();
    (exit_code, final_output)
}

// ===========================================================================
// LuaOp execution — dispatch to a plugin-registered Lua handler
// ===========================================================================
//
// Return shape mirrors `run_command` so the caller can treat both uniformly:
//   · exit_code = None or Some(non-zero) → Failed
//   · exit_code = Some(0)                → Success
//   · output                            → lines captured (logged per step)
//
// Resolution order for the target plugin name:
//   1. `op.plugin` explicit override
//   2. `default_plugin` (the pipeline's `plugin` field — the common case)
//
// Handler contract (Lua side):
//   local handler = function(params, ctx)
//     -- `params` is the JSON payload the step def carried.
//     -- `ctx` has `{ cwd = "...", plugin = "..." }` for convenience.
//     return { exit_code = 0, stdout = "…", stderr = "…" }
//   end
//
// `stdout` / `stderr` are split on newlines and appended to the step output
// (stderr lines prefixed with `[stderr]` to match shell runs). If the handler
// raises, the step is marked Failed with the error message captured.
fn run_lua_op(
    op:             &LuaOpSpec,
    cwd:            &str,
    app_handle:     &tauri::AppHandle,
    default_plugin: &str,
    sink:           &StepLogSink,
) -> (Option<i32>, Vec<String>) {
    let target_plugin = op.plugin.clone().unwrap_or_else(|| default_plugin.to_string());
    let state = app_handle.state::<crate::AppState>();
    let host = match state.plugin_host.lock() {
        Ok(h) => h,
        Err(_) => {
            let msg = "⚠ plugin host mutex poisoned".to_string();
            sink.emit(&msg);
            return (Some(1), vec![msg]);
        }
    };
    // Lua handlers return stdout/stderr as opaque blobs — the live stream is
    // therefore "burst" rather than truly per-line, but feeding each parsed
    // line through `sink` keeps the UX uniform with shell steps and ensures
    // every line lands in plugin-log + run.log as it does for `run_command`.
    match host.invoke_pipeline_op(&target_plugin, &op.op, &op.params, cwd) {
        Ok(result) => {
            let mut lines = Vec::new();
            if !result.stdout.is_empty() {
                for l in result.stdout.lines().take(500) {
                    sink.emit(l);
                    lines.push(l.to_string());
                }
            }
            if !result.stderr.is_empty() {
                for l in result.stderr.lines().take(500) {
                    let line = format!("[stderr] {l}");
                    sink.emit(&line);
                    lines.push(line);
                }
            }
            (Some(result.exit_code), lines)
        }
        Err(e) => {
            let msg = format!("⚠ lua_op '{}.{}' error: {e}", target_plugin, op.op);
            sink.emit(&msg);
            (Some(1), vec![msg])
        }
    }
}
