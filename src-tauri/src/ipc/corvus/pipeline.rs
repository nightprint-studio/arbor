//! `pipeline` (local engine) domain — all handlers routed through the
//! in-process broker.
//!
//! Queries + cancel (list definitions/runs, fetch one run, cancel, lock probe)
//! are the synchronous, `AppState`-only slice: they lock the engine registry
//! (and notify the engine condvar on cancel) and return.
//!
//! The execution handlers (`run_pipeline`, `request_pipeline_run`,
//! `resume_pipeline_run`, `discard_pipeline_run`) start/resume the orchestrator
//! thread, which takes an injected [`crate::pipeline::PipelineRuntime`] (built
//! here via [`AppState::pipeline_runtime`]) instead of reaching `AppState`
//! through a `tauri::AppHandle` on the worker thread.
//!
//! The CI-provider REST handlers (GitHub Actions / GitLab CI) live in
//! [`crate::ipc::corvus::ci`]; this is the local plugin-defined engine.

use crate::error::{AppError, Result};
use crate::ipc::corvus;
use crate::pipeline::{PipelineDef, PipelineRun};
use crate::AppState;

/// List all pipeline definitions registered by plugins.
#[corvus::handler]
fn list_pipeline_defs(state: &AppState) -> Result<Vec<PipelineDef>> {
    let reg = state.lock_pipelines()?;
    Ok(reg.defs.clone())
}

/// List all pipeline runs (most recent last).
#[corvus::handler]
fn list_pipeline_runs(state: &AppState) -> Result<Vec<PipelineRun>> {
    let reg = state.lock_pipelines()?;
    Ok(reg.runs.iter().rev().cloned().collect())
}

/// Get a single pipeline run by ID.
#[corvus::handler]
fn get_pipeline_run(state: &AppState, run_id: String) -> Result<PipelineRun> {
    let reg = state.lock_pipelines()?;
    reg.get_run(&run_id)
        .cloned()
        .ok_or_else(|| AppError::Other(format!("pipeline run '{run_id}' not found")))
}

/// Cancel a running pipeline (stops after the current step completes). Also
/// wakes any orchestrator parked on the global concurrency condvar so a queued
/// run's cancel takes effect within microseconds rather than waiting out the
/// 250 ms poll timeout.
#[corvus::handler]
fn cancel_pipeline_run(state: &AppState, run_id: String) -> Result<()> {
    {
        let mut reg = state.lock_pipelines()?;
        reg.cancel(&run_id);
    }
    state.pipeline_engine.cv.notify_all();
    Ok(())
}

/// Return the run_id currently holding `lock_key`, or `None` when the lock is
/// free. Used by plugins/UI to pre-flight "can I start?" checks.
#[corvus::handler]
fn is_pipeline_locked(state: &AppState, lock_key: String) -> Result<Option<String>> {
    let reg = state.lock_pipelines()?;
    Ok(reg.locked_by(&lock_key).map(String::from))
}

// ---------------------------------------------------------------------------
// Execution handlers — drive the orchestrator thread via an injected runtime.
// ---------------------------------------------------------------------------

/// Build the orchestrator runtime from `state`, or a clear IPC error when the
/// event sink isn't wired yet (only possible during early boot).
fn pipeline_runtime(state: &AppState) -> Result<crate::pipeline::PipelineRuntime> {
    state.pipeline_runtime()
        .ok_or_else(|| AppError::Other("pipeline runtime unavailable".into()))
}

/// Shared body for `run_pipeline` and the direct path of `request_pipeline_run`:
/// resolve the def + repo path, seed an initial run, and spawn the orchestrator.
fn start_run(
    state:       &AppState,
    plugin:      String,
    pipeline_id: String,
    tab_id:      Option<String>,
) -> Result<String> {
    // Find the definition.
    let def = {
        let reg = state.lock_pipelines()?;
        reg.defs.iter()
            .find(|d| d.id == pipeline_id && d.plugin == plugin)
            .cloned()
            .ok_or_else(|| AppError::Other(
                format!("pipeline '{pipeline_id}' not found in plugin '{plugin}'")
            ))?
    };

    // Resolve repo path for cwd fallback (forward handler — asking corvus-be is
    // safe here; the launcher holds no repo registry).
    let repo_path = tab_id
        .as_deref()
        .and_then(|tid| crate::ipc::resolve_tab_path(state, tid).ok());

    // Build initial run state (all steps Pending) seeded with lock_key + log_level.
    let run_id = {
        let mut reg = state.lock_pipelines()?;
        reg.new_run_id()
    };
    let run = def.new_run(run_id.clone(), repo_path.clone());

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut reg = state.lock_pipelines()?;
        reg.add_run(run, cancel.clone());
    }

    // Start the orchestrator thread.
    let rt = pipeline_runtime(state)?;
    crate::pipeline::start_pipeline_run(
        def, run_id.clone(), repo_path, cancel, std::sync::Arc::new(rt),
    );

    Ok(run_id)
}

/// Start a pipeline run. Returns the run ID.
/// `tab_id` is used to look up the active repo's working directory.
#[corvus::handler]
fn run_pipeline(
    state:       &AppState,
    plugin:      String,
    pipeline_id: String,
    tab_id:      Option<String>,
) -> Result<String> {
    start_run(state, plugin, pipeline_id, tab_id)
}

/// Request a pipeline run for a def the user picked from the panel.
///
/// A pipeline def is *self-contained* when its `stages` array is non-empty:
/// every step has its command / op / cwd already resolved (variable
/// substitution baked in) so the orchestrator can replay it without any
/// plugin context. We run those directly — same as `run_pipeline` — so a
/// def compiled by a previous run (sequence, combo button, …) keeps
/// working from the panel even when the user has switched tabs and the
/// owning plugin would not be able to recompute the context.
///
/// A def with empty `stages` is a *stub* the plugin registered upfront so
/// the panel has something to show. Stubs cannot be replayed verbatim — we
/// delegate to the plugin's `on_pipeline_run_request` hook so it can
/// materialise stages (typically by re-compiling a profile or run config)
/// and call `arbor.pipeline.run` itself. If the plugin has no such hook,
/// we surface a clear error rather than spawning a 0-step ghost run.
#[corvus::handler]
fn request_pipeline_run(
    state:       &AppState,
    plugin:      String,
    pipeline_id: String,
    tab_id:      Option<String>,
) -> Result<Option<String>> {
    // Look up the def to decide which path to take. Cloning is cheap (a
    // pipeline def is just a few stages of metadata) and we want to drop
    // the registry lock before firing into Lua.
    let def_stages_empty = {
        let reg = state.lock_pipelines()?;
        reg.defs.iter()
            .find(|d| d.id == pipeline_id && d.plugin == plugin)
            .map(|d| d.stages.is_empty())
    };

    match def_stages_empty {
        // Def found, has stages → run directly (self-contained).
        Some(false) => {
            let run_id = start_run(state, plugin, pipeline_id, tab_id)?;
            Ok(Some(run_id))
        }
        // Def found, but stages are empty → must route through the plugin.
        Some(true) => {
            let host = state.lock_plugin_host()?;
            if host.plugin_has_handler(&plugin, "on_pipeline_run_request") {
                let ctx = serde_json::json!({
                    "pipeline_id": pipeline_id,
                    "tab_id":      tab_id,
                }).to_string();
                arbor_plugin_core::prelude::fire_on(&host, &plugin, "on_pipeline_run_request", &ctx);
                Ok(None)
            } else {
                Err(AppError::Other(format!(
                    "pipeline '{pipeline_id}' is a placeholder — its owning plugin '{plugin}' has no `on_pipeline_run_request` hook to compile it. Launch it from the plugin's own UI first."
                )))
            }
        }
        // Def not found at all → standard not-found error from start_run.
        None => {
            let run_id = start_run(state, plugin, pipeline_id, tab_id)?;
            Ok(Some(run_id))
        }
    }
}

/// Resume a failed/paused pipeline run from the step(s) that halted it.
#[corvus::handler]
fn resume_pipeline_run(state: &AppState, run_id: String) -> Result<()> {
    let rt = pipeline_runtime(state)?;
    crate::pipeline::resume_run(&run_id, std::sync::Arc::new(rt)).map_err(AppError::Other)
}

/// Drop a terminal (failed/cancelled/success) run from the registry and disk.
#[corvus::handler]
fn discard_pipeline_run(state: &AppState, run_id: String) -> Result<()> {
    let rt = pipeline_runtime(state)?;
    crate::pipeline::discard_run(&run_id, std::sync::Arc::new(rt)).map_err(AppError::Other)
}
