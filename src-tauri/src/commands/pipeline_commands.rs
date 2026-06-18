use tauri::State;

use crate::AppState;
use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Execution commands
//
// The query + cancel + lock-probe handlers (list_pipeline_defs,
// list_pipeline_runs, get_pipeline_run, cancel_pipeline_run, is_pipeline_locked)
// are sync `AppState`-only and migrated to `crate::ipc::corvus::pipeline`. The
// commands below stay inline: they drive the orchestrator thread, which reaches
// `AppState` through the `AppHandle` (`app.state::<AppState>()`) on the worker
// thread — not just for emit — so they await the runtime-context refactor.
// ---------------------------------------------------------------------------

/// Start a pipeline run. Returns the run ID.
/// `tab_id` is used to look up the active repo's working directory.
#[tauri::command]
pub fn run_pipeline(
    state:      State<AppState>,
    app_handle: tauri::AppHandle,
    plugin:     String,
    pipeline_id: String,
    tab_id:     Option<String>,
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

    // Resolve repo path for cwd fallback.
    let repo_path = tab_id.as_deref().and_then(|tid| {
        state.repos.lock().ok().and_then(|mut mgr| {
            mgr.get(tid).ok().map(|r| r.path.clone())
        })
    });

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
    crate::pipeline::start_pipeline_run(def, run_id.clone(), repo_path, cancel, app_handle);

    Ok(run_id)
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
#[tauri::command]
pub fn request_pipeline_run(
    state:      State<AppState>,
    app_handle: tauri::AppHandle,
    plugin:     String,
    pipeline_id: String,
    tab_id:     Option<String>,
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
            let run_id = run_pipeline(state, app_handle, plugin, pipeline_id, tab_id)?;
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
        // Def not found at all → standard not-found error from run_pipeline.
        None => {
            let run_id = run_pipeline(state, app_handle, plugin, pipeline_id, tab_id)?;
            Ok(Some(run_id))
        }
    }
}

/// Resume a failed/paused pipeline run from the step(s) that halted it.
#[tauri::command]
pub fn resume_pipeline_run(app_handle: tauri::AppHandle, run_id: String) -> Result<()> {
    crate::pipeline::resume_run(&run_id, app_handle).map_err(AppError::Other)
}

/// Drop a terminal (failed/cancelled/success) run from the registry and disk.
#[tauri::command]
pub fn discard_pipeline_run(app_handle: tauri::AppHandle, run_id: String) -> Result<()> {
    crate::pipeline::discard_run(&run_id, app_handle).map_err(AppError::Other)
}
