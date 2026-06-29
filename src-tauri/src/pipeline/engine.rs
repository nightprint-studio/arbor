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

use arbor_plugin_core::prelude::{PipelineOpResult, PluginHost};

use super::PipelineRegistry;
use crate::plugin_logs::PluginLogBuffer;

/// Worker-thread dispatch into a backend (out-of-process) plugin VM for a
/// `lua_op` step. After the per-product plugin flip, plugins register their
/// pipeline ops in the corvus-be VM, so the shell `PluginHost` no longer knows
/// the op; the orchestrator falls back to this closure, which routes the call
/// to the backend's `invoke_pipeline_op` RPC and blocks on the reply.
///
/// Built from `&AppState` at run start (it captures an `Arc<Router>`, never an
/// `AppHandle`), so it is `Send + Sync` and safe to call on the worker thread.
/// Signature: `(plugin_name, op, params, cwd) -> PipelineOpResult`.
pub type BeLuaOpDispatch =
    Arc<dyn Fn(&str, &str, serde_json::Value, &str) -> Result<PipelineOpResult, String> + Send + Sync>;

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
    /// Backend-VM fallback for `lua_op` steps whose op the shell host doesn't
    /// know (registered out-of-process in corvus-be). `None` when no router is
    /// wired yet; `run_lua_op` tries the shell host first and only reaches here
    /// when the op is "not registered". See [`BeLuaOpDispatch`].
    pub be_lua_op: Option<BeLuaOpDispatch>,
    pub plugin_logs: Arc<Mutex<PluginLogBuffer>>,
    pub max_concurrent_runs: u32,
}

impl PipelineRuntime {
    /// Fire a fire-and-forget plugin hook (replicates `AppState::fire_hook`).
    pub fn fire_hook(&self, hook: &str, ctx: serde_json::Value) {
        self.hooks.fire_blocking(hook, PluginValue::from_json(ctx));
    }
}
