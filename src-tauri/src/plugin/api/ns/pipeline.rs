//! `arbor.pipeline` — pipeline management.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let pipeline_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_define(ctx, lua, &pipeline_table)?;
    install_run(ctx, lua, &pipeline_table)?;
    install_resume(ctx, lua, &pipeline_table)?;
    install_discard(ctx, lua, &pipeline_table)?;
    install_is_locked(ctx, lua, &pipeline_table)?;
    install_list(ctx, lua, &pipeline_table)?;
    install_get(ctx, lua, &pipeline_table)?;
    install_cancel(ctx, lua, &pipeline_table)?;
    install_list_runs(ctx, lua, &pipeline_table)?;
    install_get_run(ctx, lua, &pipeline_table)?;
    install_register_op(lua, &pipeline_table)?;
    install_unregister_op(lua, &pipeline_table)?;
    install_list_ops(ctx, lua, &pipeline_table)?;

    arbor.set("pipeline", pipeline_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_define(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // define(config) — register a pipeline definition
    // config: {
    //   id, name, description?, icon?,
    //   lock_key? = string   -- default "<plugin>:<id>"; only one run per key may be Running
    //   log_level? = "debug"|"info"|"warn"|"error" -- default "info"
    //   silent?    = bool    -- default false; when true, suppress the host's
    //                        --   automatic start-toast / done-notification
    //                        --   for runs of this pipeline (per-run override
    //                        --   available via arbor.pipeline.run{silent=…})
    //   stages = [{
    //     id, name,
    //     mode? = "sequential"|"parallel" -- default sequential
    //     max_parallel? = number          -- applies when mode=parallel
    //     steps = [{ id, name, command, cwd?, allow_failure? }]
    //   }]
    // }
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, config: mlua::Table| {
        let Some(ref h) = handle else {
            return Err(mlua::Error::RuntimeError(
                "arbor.pipeline.define requires a running app handle".to_string(),
            ));
        };
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.pipeline.define: 'id' is required".to_string())
        })?;
        let name = config.get::<String>("name").map_err(|_| {
            mlua::Error::RuntimeError("arbor.pipeline.define: 'name' is required".to_string())
        })?;
        let description: Option<String> = config.get::<Option<String>>("description").unwrap_or(None);
        let icon: Option<String> = config.get::<Option<String>>("icon").unwrap_or(None);
        let lock_key: Option<String> = config.get::<Option<String>>("lock_key").unwrap_or(None);
        let log_level = crate::pipeline::parse_log_level(
            config.get::<Option<String>>("log_level").unwrap_or(None).as_deref()
        );
        let silent: bool = config.get::<Option<bool>>("silent").unwrap_or(None).unwrap_or(false);

        let stages_lua: mlua::Table = config.get::<mlua::Table>("stages").map_err(|_| {
            mlua::Error::RuntimeError("arbor.pipeline.define: 'stages' is required".to_string())
        })?;

        let stages = parse_stages(lua_ctx, stages_lua)?;

        let def = crate::pipeline::PipelineDef {
            id:       id.clone(),
            name,
            plugin:   pname.clone(),
            description, icon, stages,
            lock_key, log_level, silent,
        };
        let def_id_for_event = id.clone();
        let state = h.state::<crate::AppState>();
        state.pipelines.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
            .register_def(def);
        // Notify the frontend so caches / panels refresh without the
        // user having to click Reload. Without this, the panel shows
        // whatever list it captured at first-load — typically empty
        // when the plugin registers defs on `on_repo_open` (after the
        // panel has already mounted).
        let _ = h.emit("arbor://pipeline-def-registered",
            serde_json::json!({ "pipeline_id": def_id_for_event, "plugin": pname }));
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("define", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn parse_stages(lua_ctx: &Lua, stages_lua: mlua::Table) -> mlua::Result<Vec<crate::pipeline::StageDef>> {
    let mut stages = Vec::new();
    for stage_val in stages_lua.sequence_values::<mlua::Table>() {
        let stage_tbl = stage_val.map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let stage_id   = stage_tbl.get::<String>("id").unwrap_or_default();
        let stage_name = stage_tbl.get::<String>("name").unwrap_or_else(|_| stage_id.clone());
        let mode = crate::pipeline::parse_stage_mode(
            stage_tbl.get::<Option<String>>("mode").unwrap_or(None).as_deref()
        );
        let max_parallel: Option<usize> = stage_tbl.get::<Option<usize>>("max_parallel").unwrap_or(None);

        let steps_lua: mlua::Table = stage_tbl.get::<mlua::Table>("steps")
            .unwrap_or_else(|_| lua_ctx.create_table().unwrap());
        let steps = parse_steps(lua_ctx, steps_lua)?;
        stages.push(crate::pipeline::StageDef {
            id: stage_id, name: stage_name, steps, mode, max_parallel,
        });
    }
    Ok(stages)
}

fn parse_steps(lua_ctx: &Lua, steps_lua: mlua::Table) -> mlua::Result<Vec<crate::pipeline::StepDef>> {
    let mut steps = Vec::new();
    for step_val in steps_lua.sequence_values::<mlua::Table>() {
        let step_tbl = step_val.map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let step_id = step_tbl.get::<String>("id").unwrap_or_default();
        // Accept EITHER `command` (shell) OR `lua_op` (native).
        // Precedence: lua_op wins when both are provided.
        let command: String = step_tbl.get::<String>("command").unwrap_or_default();
        let lua_op: Option<crate::pipeline::LuaOpSpec> = match step_tbl.get::<Option<mlua::Table>>("lua_op").unwrap_or(None) {
            Some(op_tbl) => {
                let op_name = op_tbl.get::<String>("op").map_err(|_| mlua::Error::RuntimeError(
                    format!("pipeline step '{step_id}': lua_op.op is required")
                ))?;
                let plugin_override: Option<String> = op_tbl.get::<Option<String>>("plugin").unwrap_or(None);
                // params can be any serialisable Lua value; default {}.
                let params_lua: mlua::Value = op_tbl.get::<mlua::Value>("params").unwrap_or(mlua::Value::Nil);
                let params_json: serde_json::Value = lua_ctx.from_value(params_lua)
                    .unwrap_or(serde_json::Value::Null);
                Some(crate::pipeline::LuaOpSpec {
                    plugin: plugin_override,
                    op:     op_name,
                    params: params_json,
                })
            }
            None => None,
        };
        // Optional `env` map for shell `command` steps. Ignored for lua_op steps
        // (handlers own their env). Lua callers pass a plain table of strings.
        let env: std::collections::HashMap<String, String> = step_tbl
            .get::<Option<mlua::Table>>("env")
            .unwrap_or(None)
            .map(|t| {
                let mut m = std::collections::HashMap::new();
                for pair in t.pairs::<String, String>() {
                    if let Ok((k, v)) = pair { m.insert(k, v); }
                }
                m
            })
            .unwrap_or_default();

        // Optional builtin / if_block / capture — deserialised via serde
        // from a Lua table → JSON pivot. The types live in
        // crate::pipeline::{builtin, condition, vars} and use tagged
        // representations so plugins write
        //   builtin = { kind = "file_exists", path = "..." }
        // and the right enum variant lands.
        let builtin: Option<crate::pipeline::BuiltinSpec> =
            parse_optional(&step_tbl, "builtin", lua_ctx)
                .map_err(|e| mlua::Error::RuntimeError(format!(
                    "pipeline step '{step_id}': builtin parse error — {e}"
                )))?;
        let if_block: Option<crate::pipeline::IfBlock> =
            parse_optional(&step_tbl, "if_block", lua_ctx)
                .map_err(|e| mlua::Error::RuntimeError(format!(
                    "pipeline step '{step_id}': if_block parse error — {e}"
                )))?;
        let capture: Option<crate::pipeline::CaptureSpec> =
            parse_optional(&step_tbl, "capture", lua_ctx)
                .map_err(|e| mlua::Error::RuntimeError(format!(
                    "pipeline step '{step_id}': capture parse error — {e}"
                )))?;

        if command.is_empty() && lua_op.is_none()
            && builtin.is_none() && if_block.is_none()
        {
            return Err(mlua::Error::RuntimeError(format!(
                "pipeline step '{step_id}': one of 'command', 'lua_op', \
                 'builtin' or 'if_block' is required"
            )));
        }

        steps.push(crate::pipeline::StepDef {
            id:      step_id,
            name:    step_tbl.get::<String>("name").unwrap_or_default(),
            command,
            lua_op,
            builtin,
            if_block,
            cwd:          step_tbl.get::<Option<String>>("cwd").unwrap_or(None),
            allow_failure: step_tbl.get::<bool>("allow_failure").unwrap_or(false),
            env,
            capture,
        });
    }
    Ok(steps)
}

/// Read an optional struct-typed field from a Lua step table by pivoting
/// through `serde_json::Value` (Lua → JSON → typed). Returns `None` when
/// the key is absent or `nil`. We deliberately use the JSON pivot (instead
/// of `from_value` directly into `T`) so that recursive types like
/// `IfBlock` go through the same serialisation as the persistent on-disk
/// representation, side-stepping mlua-direct edge cases around tagged
/// enums and empty arrays.
fn parse_optional<T>(
    tbl:     &mlua::Table,
    key:     &str,
    lua_ctx: &mlua::Lua,
) -> std::result::Result<Option<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    let raw: mlua::Value = match tbl.get::<mlua::Value>(key) {
        Ok(v)  => v,
        Err(_) => return Ok(None),
    };
    if matches!(raw, mlua::Value::Nil) { return Ok(None); }
    let json: serde_json::Value = lua_ctx.from_value(raw).map_err(|e| e.to_string())?;
    if json.is_null() { return Ok(None); }
    serde_json::from_value::<T>(json).map(Some).map_err(|e| e.to_string())
}

fn install_run(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // run{pipeline_id, cwd?, silent?} → (run_id, nil) | (nil, err)
    //   silent? — when true, suppresses the host's automatic start-toast and
    //             done-notification for THIS run, regardless of the def's
    //             default. Pass `false` to force the toast/notify even when
    //             the def was registered with `silent = true`.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        let pipeline_id: String = cfg.get("pipeline_id").map_err(|_|
            mlua::Error::RuntimeError("arbor.pipeline.run: 'pipeline_id' is required".into()))?;
        let override_cwd: Option<String> = cfg.get::<Option<String>>("cwd").unwrap_or(None);
        let silent_override: Option<bool> = cfg.get::<Option<bool>>("silent").unwrap_or(None);

        let Some(ref h) = handle else {
            return err2(lua_ctx, "pipeline.run: app handle unavailable");
        };
        let state = h.state::<crate::AppState>();

        let def = {
            let reg = match state.pipelines.lock() {
                Ok(g)  => g,
                Err(e) => return err2(lua_ctx, format!("pipeline.run lock: {e}")),
            };
            match reg.defs.iter().find(|d| d.id == pipeline_id && d.plugin == pname).cloned() {
                Some(d) => d,
                None    => return err2(lua_ctx,
                    format!("pipeline.run: pipeline '{pipeline_id}' not found")),
            }
        };

        let repo_path = override_cwd.or_else(|| {
            state.active_tab_id.lock().ok()
                .and_then(|tid| tid.clone())
                .and_then(|tid| {
                    state.repos.lock().ok().and_then(|mut mgr| {
                        mgr.get(&tid).ok().map(|r| r.path.clone())
                    })
                })
        });

        let run_id = match state.pipelines.lock() {
            Ok(mut reg) => reg.new_run_id(),
            Err(e)      => return err2(lua_ctx, format!("pipeline.run lock: {e}")),
        };
        let mut run = def.new_run(run_id.clone(), repo_path.clone());
        if let Some(s) = silent_override {
            run.silent = s;
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        match state.pipelines.lock() {
            Ok(mut reg) => reg.add_run(run, cancel.clone()),
            Err(e)      => return err2(lua_ctx, format!("pipeline.run add_run: {e}")),
        }

        crate::pipeline::start_pipeline_run(def, run_id.clone(), repo_path, cancel, h.clone());
        ok2(lua_ctx, run_id)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("run", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_resume(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // resume(run_id) → (true, nil) | (false, err)
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, run_id: String| -> LuaTuple {
        let Some(ref h) = handle else {
            return boolerr2(lua_ctx, false, Some("pipeline.resume: app handle unavailable".into()));
        };
        match crate::pipeline::resume_run(&run_id, h.clone()) {
            Ok(_)  => boolerr2(lua_ctx, true, None),
            Err(e) => boolerr2(lua_ctx, false, Some(format!("pipeline.resume: {e}"))),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("resume", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_discard(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // discard(run_id) → (true, nil) | (false, err)
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, run_id: String| -> LuaTuple {
        let Some(ref h) = handle else {
            return boolerr2(lua_ctx, false, Some("pipeline.discard: app handle unavailable".into()));
        };
        match crate::pipeline::discard_run(&run_id, h.clone()) {
            Ok(_)  => boolerr2(lua_ctx, true, None),
            Err(e) => boolerr2(lua_ctx, false, Some(format!("pipeline.discard: {e}"))),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("discard", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_locked(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // is_locked(lock_key) → run_id | nil
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, lock_key: String| {
        let Some(ref h) = handle else { return Ok(mlua::Value::Nil); };
        let state = h.state::<crate::AppState>();
        let reg   = state.pipelines.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match reg.locked_by(&lock_key) {
            Some(id) => Ok(mlua::Value::String(lua_ctx.create_string(id.as_bytes())?)),
            None     => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("is_locked", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // list() → array of pipeline definitions for this plugin
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, ()| {
        let Some(ref h) = handle else {
            return Ok(lua_ctx.create_table()? as Table);
        };
        let state = h.state::<crate::AppState>();
        let reg   = state.pipelines.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let defs: Vec<_> = reg.defs.iter()
            .filter(|d| d.plugin == pname)
            .collect();
        let json = serde_json::to_value(&defs)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match lua_ctx.to_value(&json) {
            Ok(mlua::Value::Table(t)) => Ok(t),
            _ => lua_ctx.create_table(),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // get(id) → pipeline definition table for THIS plugin, or nil. Scoped
    // to the calling plugin so a plugin can't peek at another's defs (and
    // accidentally collide on a shared id like `"build"`).
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, id: String| {
        let Some(ref h) = handle else { return Ok(mlua::Value::Nil); };
        let state = h.state::<crate::AppState>();
        let reg   = state.pipelines.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let Some(def) = reg.defs.iter().find(|d| d.id == id && d.plugin == pname)
        else { return Ok(mlua::Value::Nil); };
        let json = serde_json::to_value(def)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        Ok(lua_ctx.to_value(&json).unwrap_or(mlua::Value::Nil))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("get", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, run_id: String| {
        if let Some(ref h) = handle {
            let state = h.state::<crate::AppState>();
            if let Ok(mut reg) = state.pipelines.lock() {
                reg.cancel(&run_id);
            };
            // Wake any orchestrator parked on the concurrency condvar so a
            // queued run's cancel lands instantly instead of after the
            // 250 ms poll tick.
            state.pipeline_cv.notify_all();
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("cancel", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_runs(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // list_runs(opts?) → array of pipeline runs
    // opts.plugin      — filter by plugin name (defaults to current plugin)
    // opts.pipeline_id — additionally filter by pipeline id
    // opts.all         — when true, returns runs from every plugin (ignores plugin filter)
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<mlua::Table>| {
        let Some(ref h) = handle else {
            return Ok(lua_ctx.create_table()? as Table);
        };
        let state = h.state::<crate::AppState>();
        let reg   = state.pipelines.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

        let filter_plugin:      Option<String> = opts.as_ref()
            .and_then(|t| t.get::<Option<String>>("plugin").unwrap_or(None));
        let filter_pipeline_id: Option<String> = opts.as_ref()
            .and_then(|t| t.get::<Option<String>>("pipeline_id").unwrap_or(None));
        let all: bool = opts.as_ref()
            .and_then(|t| t.get::<Option<bool>>("all").unwrap_or(None))
            .unwrap_or(false);
        let plugin_scope = if all { None }
            else { Some(filter_plugin.unwrap_or_else(|| pname.clone())) };

        let runs: Vec<_> = reg.runs.iter()
            .filter(|r| plugin_scope.as_deref().map_or(true, |p| r.plugin == p))
            .filter(|r| filter_pipeline_id.as_deref().map_or(true, |id| r.pipeline_id == id))
            .cloned()
            .collect();

        let json = serde_json::to_value(&runs)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match lua_ctx.to_value(&json) {
            Ok(mlua::Value::Table(t)) => Ok(t),
            _ => lua_ctx.create_table(),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("list_runs", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get_run(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, run_id: String| {
        let Some(ref h) = handle else { return Ok(mlua::Value::Nil); };
        let state = h.state::<crate::AppState>();
        let reg   = state.pipelines.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        match reg.get_run(&run_id) {
            Some(r) => {
                let json = serde_json::to_value(r)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                lua_ctx.to_value(&json)
            },
            None => Ok(mlua::Value::Nil),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("get_run", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register_op(lua: &Lua, t: &Table) -> Result<()> {
    // arbor.pipeline.register_op(name, handler)
    //
    // Registers a Lua function under `__arbor_pipeline_ops__[name]` on the
    // plugin's globals. The pipeline orchestrator calls it when a step has
    // `lua_op = { op = "<name>", params = ... }` instead of a shell
    // `command`. The handler signature is:
    //
    //   function(params, ctx) -> nil | bool | int | string | table
    //
    //   ctx.cwd    — step's resolved working directory
    //   ctx.plugin — plugin name (useful when the same handler serves
    //                multiple plugins via `arbor.service`)
    //
    // Return shapes are permissive (see `coerce_pipeline_op_result` in
    // runtime.rs): `nil` / `true` → success; `false` / integer → that
    // exit code; `string` → stdout with exit=0; `table` with
    // `{ exit_code?, stdout?, stderr? }` for the structured form.
    // Raising an error fails the step with the message captured.
    let fn_ = lua.create_function(|lua_ctx, (name, handler): (String, mlua::Function)| {
        let reg: mlua::Table = match lua_ctx.globals().get("__arbor_pipeline_ops__") {
            Ok(t) => t,
            Err(_) => {
                let t = lua_ctx.create_table()?;
                lua_ctx.globals().set("__arbor_pipeline_ops__", t.clone())?;
                t
            }
        };
        reg.set(name, handler)?;
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("register_op", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_unregister_op(lua: &Lua, t: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, name: String| {
        if let Ok(reg) = lua_ctx.globals().get::<mlua::Table>("__arbor_pipeline_ops__") {
            let _ = reg.set(name, mlua::Value::Nil);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("unregister_op", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_ops(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // arbor.pipeline.list_ops() — debugging helper: all registered ops
    // across all enabled plugins, as "<plugin>.<op>" strings.
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ()| {
        let h = match &handle {
            Some(h) => h,
            None => return Ok(mlua::Value::Nil),
        };
        let state = h.state::<crate::AppState>();
        let ops = match state.plugin_host.lock() {
            Ok(host) => host.list_all_pipeline_ops(),
            Err(_)   => Vec::new(),
        };
        lua_ctx.to_value(&ops)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("list_ops", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
