//! `arbor.pipeline` (pipeline definition + run orchestration), ported to run
//! through an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface mirrors the shell's `ns_shell/pipeline.rs`: same namespace
//! (`arbor.pipeline`), same function names (`define` / `run` / `resume` /
//! `discard` / `is_locked` / `list` / `get` / `cancel` / `list_runs` / `get_run`
//! / `register_op` / `unregister_op` / `list_ops`), same argument shapes, same
//! `(value, err)` / `(true|false, err)` tuple conventions, same validation
//! `RuntimeError` strings, same `pipeline.<op>: …` error prefixes.
//!
//! This is a **PROXY** namespace: the `PipelineEngine` / `PipelineRuntime` (the
//! registry of defs + runs and the orchestrator threads that drive the OS
//! processes) lives in the SHELL's `AppState` (`pipeline_engine`), not in
//! `corvus-be`. So every host-touching op round-trips through the captured
//! `Arc<dyn NsHost>` whose `corvus-be` impl calls back over the reverse channel
//! (`host_call("__pipeline_<op>", …)`); the matching shell handlers in
//! `src-tauri/src/ipc/mod.rs` read/mutate the real engine + start/resume/discard
//! runs exactly as `ns_shell/pipeline.rs` did. The plugin is identified by
//! `ctx.plugin_name` (forwarded on every op that scopes by plugin, exactly as the
//! shell's `ctx.plugin_name`).
//!
//! Two ops stay **purely Lua-local** (no host round-trip), byte-for-byte with the
//! shell: `register_op` / `unregister_op` mutate the `__arbor_pipeline_ops__`
//! global table on the plugin's own VM. The orchestrator that consumes those ops
//! lives shell-side, so a `lua_op` step registered on a corvus-be VM is only
//! reachable when the run executes inside a VM that holds the same table — see
//! the callback-delivery gap below.
//!
//! ## Callback / cross-process gap (flagged)
//!
//! The shell can only push run lifecycle (`arbor://pipeline-*` events, log
//! streams) and, crucially, **`lua_op` step dispatch** into the VM that owns the
//! orchestrator. corvus-be VMs are a separate process with no shell→BE push
//! channel for synchronous op dispatch, so:
//!   · `register_op` / `unregister_op` still populate the BE VM's
//!     `__arbor_pipeline_ops__` table (so the surface is identical), but a run
//!     started shell-side cannot call back into a BE-registered handler — those
//!     `lua_op` steps will not resolve to the BE closure. This is the same
//!     limitation as `arbor.job`'s `on_done` callback (shell callback registry is
//!     process-local).
//!   · `run` / `resume` start the orchestration; the run's progress is observable
//!     via the `arbor://pipeline-*` events the shell emits (re-broadcast), and via
//!     polling `list_runs` / `get_run`, but no per-run callback is delivered back
//!     into the corvus-be VM.
//! The start/state ops themselves proxy cleanly — it is only the async
//! callback-into-BE delivery that degrades.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    boolerr2, err2, json_to_lua, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.pipeline.*` installer. Holds the host handle the closures call through.
pub struct PipelineInstaller {
    host: NsHostHandle,
}

impl PipelineInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for PipelineInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let t = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_define(self.host.clone(), ctx, lua, &t)?;
        install_run(self.host.clone(), ctx, lua, &t)?;
        install_resume(self.host.clone(), lua, &t)?;
        install_discard(self.host.clone(), lua, &t)?;
        install_is_locked(self.host.clone(), lua, &t)?;
        install_list(self.host.clone(), ctx, lua, &t)?;
        install_get(self.host.clone(), ctx, lua, &t)?;
        install_cancel(self.host.clone(), lua, &t)?;
        install_list_runs(self.host.clone(), ctx, lua, &t)?;
        install_get_run(self.host.clone(), lua, &t)?;
        install_register_op(lua, &t)?;
        install_unregister_op(lua, &t)?;
        install_list_ops(self.host.clone(), lua, &t)?;

        arbor
            .set("pipeline", t)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_define(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    // define(config) — register a pipeline definition. The whole config table is
    // marshalled to JSON installer-side (Lua → serde_json::Value); the shell
    // handler deserializes it into the typed `PipelineDef` (injecting `plugin`),
    // registers it on the engine, and emits `arbor://pipeline-def-registered`.
    // Required-field validation that the shell did pre-parse (`id`/`name`/`stages`
    // present, each step has one of command/lua_op/builtin/if_block) runs
    // installer-side here so the raise-on-bad-shape semantics stay byte-for-byte.
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, config: mlua::Table| {
            let _id = config.get::<String>("id").map_err(|_| {
                mlua::Error::RuntimeError("arbor.pipeline.define: 'id' is required".to_string())
            })?;
            let _name = config.get::<String>("name").map_err(|_| {
                mlua::Error::RuntimeError("arbor.pipeline.define: 'name' is required".to_string())
            })?;
            // 'stages' must be present (the shell raised when absent).
            let stages: mlua::Table = config.get::<mlua::Table>("stages").map_err(|_| {
                mlua::Error::RuntimeError(
                    "arbor.pipeline.define: 'stages' is required".to_string(),
                )
            })?;
            // Mirror the shell's per-step "one of command/lua_op/builtin/if_block
            // is required" validation so a malformed def raises with the same
            // message instead of registering a no-op step.
            validate_steps(&stages)?;

            // Marshal the full config table to JSON for the wire. The shell handler
            // deserializes it into `PipelineDef` (with `plugin = pname`).
            let config_json: serde_json::Value = lua_ctx.from_value(mlua::Value::Table(config))
                .map_err(|e| {
                    mlua::Error::RuntimeError(format!("arbor.pipeline.define: encode config: {e}"))
                })?;
            host.pipeline_define(config_json, &pname)
                .map_err(mlua::Error::RuntimeError)?;
            Ok(())
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("define", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// Mirror the shell's `parse_steps` validation: every step in every stage must
/// carry one of `command` / `lua_op` / `builtin` / `if_block`. Raises with the
/// shell's exact message on the first offender.
fn validate_steps(stages: &mlua::Table) -> mlua::Result<()> {
    for stage_val in stages.clone().sequence_values::<mlua::Table>() {
        let stage_tbl = match stage_val {
            Ok(t) => t,
            Err(_) => continue,
        };
        let steps: mlua::Table = match stage_tbl.get::<mlua::Table>("steps") {
            Ok(t) => t,
            Err(_) => continue,
        };
        for step_val in steps.sequence_values::<mlua::Table>() {
            let step_tbl = match step_val {
                Ok(t) => t,
                Err(_) => continue,
            };
            let step_id = step_tbl.get::<String>("id").unwrap_or_default();
            let command: String = step_tbl.get::<String>("command").unwrap_or_default();
            let has_lua_op = !matches!(
                step_tbl.get::<mlua::Value>("lua_op").unwrap_or(mlua::Value::Nil),
                mlua::Value::Nil
            );
            let has_builtin = !matches!(
                step_tbl.get::<mlua::Value>("builtin").unwrap_or(mlua::Value::Nil),
                mlua::Value::Nil
            );
            let has_if_block = !matches!(
                step_tbl.get::<mlua::Value>("if_block").unwrap_or(mlua::Value::Nil),
                mlua::Value::Nil
            );
            if command.is_empty() && !has_lua_op && !has_builtin && !has_if_block {
                return Err(mlua::Error::RuntimeError(format!(
                    "pipeline step '{step_id}': one of 'command', 'lua_op', \
                     'builtin' or 'if_block' is required"
                )));
            }
        }
    }
    Ok(())
}

fn install_run(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    // run{pipeline_id, cwd?, silent?} → (run_id, nil) | (nil, err)
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            let pipeline_id: String = cfg.get("pipeline_id").map_err(|_| {
                mlua::Error::RuntimeError(
                    "arbor.pipeline.run: 'pipeline_id' is required".into(),
                )
            })?;
            let cwd: Option<String> = cfg.get::<Option<String>>("cwd").unwrap_or(None);
            let silent: Option<bool> = cfg.get::<Option<bool>>("silent").unwrap_or(None);

            match host.pipeline_run(&pname, &pipeline_id, cwd.as_deref(), silent) {
                Ok(run_id) => ok2(lua_ctx, run_id),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("run", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_resume(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // resume(run_id) → (true, nil) | (false, err)
    let fn_ = lua
        .create_function(move |lua_ctx, run_id: String| -> LuaTuple {
            match host.pipeline_resume(&run_id) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("resume", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_discard(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // discard(run_id) → (true, nil) | (false, err)
    let fn_ = lua
        .create_function(move |lua_ctx, run_id: String| -> LuaTuple {
            match host.pipeline_discard(&run_id) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("discard", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_locked(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // is_locked(lock_key) → run_id | nil
    let fn_ = lua
        .create_function(move |lua_ctx, lock_key: String| {
            // Host returns the holding run id, or None when the key is free
            // (→ Lua nil). A host-call error maps to nil (the shell never
            // surfaced an error here — a poisoned mutex raised, but the BE side
            // collapses that into "not locked" to keep the surface `→ run_id|nil`).
            match host.pipeline_is_locked(&lock_key) {
                Ok(Some(id)) => Ok(mlua::Value::String(lua_ctx.create_string(id.as_bytes())?)),
                _ => Ok(mlua::Value::Nil),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("is_locked", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    // list() → array of pipeline definitions for THIS plugin
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, ()| {
            // Host returns the serde-serialized `Vec<PipelineDef>` (already scoped
            // to this plugin). A host-call error collapses to an empty table,
            // mirroring the shell's "no handle → empty table" fallback.
            let json = match host.pipeline_list(&pname) {
                Ok(v) => v,
                Err(_) => return lua_ctx.create_table(),
            };
            match json_to_lua(lua_ctx, &json)? {
                mlua::Value::Table(tbl) => Ok(tbl),
                _ => lua_ctx.create_table(),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    // get(id) → pipeline definition table for THIS plugin, or nil
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, id: String| {
            // Host returns Some(def_json) or None (→ Lua nil), scoped to this
            // plugin. A host-call error maps to nil (mirrors the shell's
            // "no handle → nil").
            match host.pipeline_get(&pname, &id) {
                Ok(Some(json)) => json_to_lua(lua_ctx, &json),
                _ => Ok(mlua::Value::Nil),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("get", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // cancel(run_id) → nil   (best-effort; the shell's cancel never fails)
    let fn_ = lua
        .create_function(move |_lua_ctx, run_id: String| {
            let _ = host.pipeline_cancel(&run_id);
            Ok(())
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("cancel", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_runs(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    // list_runs(opts?) → array of pipeline runs
    // opts.plugin / opts.pipeline_id / opts.all (see the shell for the filter
    // semantics — all applied host-side).
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<mlua::Table>| {
            let filter_plugin: Option<String> = opts
                .as_ref()
                .and_then(|t| t.get::<Option<String>>("plugin").unwrap_or(None));
            let filter_pipeline_id: Option<String> = opts
                .as_ref()
                .and_then(|t| t.get::<Option<String>>("pipeline_id").unwrap_or(None));
            let all: bool = opts
                .as_ref()
                .and_then(|t| t.get::<Option<bool>>("all").unwrap_or(None))
                .unwrap_or(false);

            let json = match host.pipeline_list_runs(
                &pname,
                filter_plugin.as_deref(),
                filter_pipeline_id.as_deref(),
                all,
            ) {
                Ok(v) => v,
                Err(_) => return lua_ctx.create_table(),
            };
            match json_to_lua(lua_ctx, &json)? {
                mlua::Value::Table(tbl) => Ok(tbl),
                _ => lua_ctx.create_table(),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list_runs", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get_run(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // get_run(run_id) → run table | nil
    let fn_ = lua
        .create_function(move |lua_ctx, run_id: String| {
            match host.pipeline_get_run(&run_id) {
                Ok(Some(json)) => json_to_lua(lua_ctx, &json),
                _ => Ok(mlua::Value::Nil),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("get_run", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register_op(lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // register_op(name, handler) — purely Lua-local: populate this VM's
    // `__arbor_pipeline_ops__` global. Byte-for-byte with the shell. NOTE: the
    // orchestrator that dispatches these ops lives shell-side, so an op
    // registered on a corvus-be VM is only reachable from a run executing in the
    // same VM — see the module-level callback gap.
    let fn_ = lua
        .create_function(|lua_ctx, (name, handler): (String, mlua::Function)| {
            let reg: mlua::Table = match lua_ctx.globals().get("__arbor_pipeline_ops__") {
                Ok(t) => t,
                Err(_) => {
                    let t = lua_ctx.create_table()?;
                    lua_ctx
                        .globals()
                        .set("__arbor_pipeline_ops__", t.clone())?;
                    t
                }
            };
            reg.set(name, handler)?;
            Ok(())
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("register_op", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_unregister_op(lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(|lua_ctx, name: String| {
            if let Ok(reg) = lua_ctx
                .globals()
                .get::<mlua::Table>("__arbor_pipeline_ops__")
            {
                let _ = reg.set(name, mlua::Value::Nil);
            }
            Ok(())
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("unregister_op", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_ops(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // list_ops() — debugging helper: all registered ops across all enabled
    // plugins as "<plugin>.<op>" strings. The shell reads the global `PluginHost`;
    // proxied here. A host-call error collapses to nil (mirrors the shell's
    // "no handle → nil").
    let fn_ = lua
        .create_function(move |lua_ctx, ()| match host.pipeline_list_ops() {
            Ok(json) => json_to_lua(lua_ctx, &json),
            Err(_) => Ok(mlua::Value::Nil),
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list_ops", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
