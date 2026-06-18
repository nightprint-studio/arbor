// Engine state + injected runtime context for the pipeline orchestrator.
//
// These two structs are what let the orchestrator worker thread run without
// ever reaching into `AppState` / `tauri::AppHandle`:
//   · `PipelineEngine` owns the run/def registry + the concurrency condvar
//     (lifted out of the `AppState` god-object so the engine is relocatable);
//   · `PipelineRuntime` is the per-run bundle the command handlers build from
//     `&AppState` and inject at run start (engine + event sink + hooks +
//     plugin-log buffer + the concurrency cap snapshot).

use std::sync::{Arc, Condvar, Mutex};

use arbor_ipc::prelude::EventSink;
use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};

use arbor_plugin_core::prelude::PluginHost;

use super::PipelineRegistry;
use crate::plugin_logs::PluginLogBuffer;

/// Self-contained pipeline-engine state: the run/def registry plus the
/// concurrency condvar (they're used as a pair — the slot wait holds the
/// registry lock around `cv.wait_timeout`). Owned by AppState as an Arc so
/// the orchestrator worker thread shares it without reaching into AppState.
pub struct PipelineEngine {
    pub registry: Mutex<PipelineRegistry>,
    pub cv: Condvar,
}

impl PipelineEngine {
    pub fn new(registry: PipelineRegistry) -> Self {
        Self { registry: Mutex::new(registry), cv: Condvar::new() }
    }
}

/// Everything the orchestrator worker thread needs, injected at run start so
/// it never touches `AppState`/`AppHandle`. Built from `&AppState` by the
/// command handlers (see `AppState::pipeline_runtime`).
#[derive(Clone)]
pub struct PipelineRuntime {
    pub engine: Arc<PipelineEngine>,
    pub sink: Arc<dyn EventSink>,
    pub hooks: Arc<HookDispatcher>,
    /// Lua-op steps dispatch into the plugin host on the worker thread; carry
    /// the same `Arc<Mutex<PluginHost>>` AppState owns so the orchestrator
    /// never reaches back through an `AppHandle` for it.
    pub plugin_host: Arc<Mutex<PluginHost>>,
    pub plugin_logs: Arc<Mutex<PluginLogBuffer>>,
    pub max_concurrent_runs: u32,
}

impl PipelineRuntime {
    /// Fire a fire-and-forget plugin hook (replicates `AppState::fire_hook`).
    pub fn fire_hook(&self, hook: &str, ctx: serde_json::Value) {
        self.hooks.fire_blocking(hook, PluginValue::from_json(ctx));
    }
}
