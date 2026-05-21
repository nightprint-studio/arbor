//! `arbor.job` — background job spawning + introspection.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let job_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_spawn(ctx, lua, &job_table)?;
    install_list(ctx, lua, &job_table)?;
    install_cancel(ctx, lua, &job_table)?;
    install_dismiss(ctx, lua, &job_table)?;
    install_clear_finished(ctx, lua, &job_table)?;

    arbor.set("job", job_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_spawn(ctx: &ApiCtx, lua: &Lua, job_table: &Table) -> Result<()> {
    // spawn(config) → (job_id, nil) | (nil, err)
    // config: { name, command, cwd?, env?, on_done_action?, on_done? }
    // Validation problems (missing `command`, reserved category) raise.
    // Mutex / app-handle failures come back as the (nil, err) tuple.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let spawn_fn = lua
        .create_function(move |lua_ctx, config: mlua::Table| -> LuaTuple {
            let Some(ref h) = handle else {
                return err2(lua_ctx, "job.spawn: app handle unavailable");
            };

            let name    = config.get::<String>("name").unwrap_or_else(|_| "Job".to_string());
            let command = config.get::<String>("command").map_err(|_| {
                mlua::Error::RuntimeError(
                    "arbor.job.spawn: 'command' is required".to_string()
                )
            })?;
            let cwd: Option<String>  = config.get::<Option<String>>("cwd").unwrap_or(None);
            let category: Option<String> = {
                let raw = config.get::<Option<String>>("category").unwrap_or(None);
                match raw {
                    Some(c) => {
                        let trimmed = c.trim().to_string();
                        if trimmed.eq_ignore_ascii_case("system") {
                            return Err(mlua::Error::RuntimeError(
                                "arbor.job.spawn: category 'system' is reserved".to_string()
                            ));
                        }
                        if trimmed.is_empty() { None } else { Some(trimmed) }
                    }
                    None => None,
                }
            };
            let on_done_action: Option<String> =
                config.get::<Option<String>>("on_done_action").unwrap_or(None);
            let hidden: bool = config.get::<Option<bool>>("hidden").unwrap_or(None).unwrap_or(false);

            let env: Vec<(String, String)> = config
                .get::<Option<mlua::Table>>("env")
                .unwrap_or(None)
                .map(|t| {
                    let mut pairs = Vec::new();
                    for pair in t.pairs::<String, String>() {
                        if let Ok((k, v)) = pair { pairs.push((k, v)); }
                    }
                    pairs
                })
                .unwrap_or_default();

            let state  = h.state::<crate::AppState>();
            let job_id = match state.jobs.lock() {
                Ok(mut jobs) => {
                    let id = jobs.new_id();
                    let info = crate::jobs::JobInfo {
                        id:              id.clone(),
                        name:            name.clone(),
                        plugin_name:     pname.clone(),
                        command:         command.clone(),
                        started_at:      crate::jobs::JobRegistry::now_secs(),
                        status:          crate::jobs::JobStatus::Running,
                        category:        category.clone(),
                        non_cancellable: false,
                        is_system:       false,
                        finished_at:     None,
                        hidden,
                    };
                    jobs.register(info);
                    id
                }
                Err(e) => return err2(lua_ctx, format!("job.spawn jobs lock: {e}")),
            };

            let on_done_fn: Option<String> =
                if let Ok(func) = config.get::<mlua::Function>("on_done") {
                    let synthetic = format!("__job_done_{}__", job_id);
                    let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
                    let list = lua_ctx.create_table()?;
                    list.push(func)?;
                    registry.set(synthetic.clone(), list)?;
                    Some(synthetic)
                } else {
                    None
                };

            let effective_on_done = on_done_fn.or(on_done_action);

            let _ = h.emit("arbor://job-started", serde_json::json!({
                "job_id":      &job_id,
                "name":        &name,
                "plugin_name": &pname,
                "command":     &command,
                "category":    &category,
                "hidden":      hidden,
            }));

            crate::jobs::spawn_job(
                crate::jobs::JobSpawnRequest {
                    job_id:         job_id.clone(),
                    name,
                    plugin_name:    pname.clone(),
                    command,
                    cwd,
                    env,
                    on_done_action: effective_on_done,
                    category,
                },
                h.clone(),
            );

            ok2(lua_ctx, job_id)
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    job_table.set("spawn", spawn_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, job_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let list_fn = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            let Some(ref h) = handle else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };
            let state = h.state::<crate::AppState>();
            let list = match state.jobs.lock() {
                Ok(g)  => g.list(),
                Err(e) => return err2(lua_ctx, format!("job.list lock: {e}")),
            };
            let json = match serde_json::to_value(&list) {
                Ok(v)  => v,
                Err(e) => return err2(lua_ctx, format!("job.list encode: {e}")),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("job.list to_value: {e}")),
            }
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    job_table.set("list", list_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(ctx: &ApiCtx, lua: &Lua, job_table: &Table) -> Result<()> {
    // cancel(job_id) → nil   (best-effort, never fails)
    let handle = ctx.app_handle.clone();
    let cancel_fn = lua
        .create_function(move |_lua_ctx, job_id: String| {
            let Some(ref h) = handle else { return Ok(()); };
            let state = h.state::<crate::AppState>();
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.cancel(&job_id);
            }
            Ok(())
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    job_table.set("cancel", cancel_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_dismiss(ctx: &ApiCtx, lua: &Lua, job_table: &Table) -> Result<()> {
    // dismiss(job_id) → bool   (true if removed; false if running / unknown)
    // Mirrors the host's `dismiss_job` Tauri command — only terminal jobs
    // (completed / failed / cancelled) are eligible. Running jobs are
    // ignored so a misclick doesn't leak a process from the registry.
    let handle = ctx.app_handle.clone();
    let dismiss_fn = lua
        .create_function(move |_lua_ctx, job_id: String| {
            let Some(ref h) = handle else { return Ok(false); };
            let state = h.state::<crate::AppState>();
            let dismissed = if let Ok(mut jobs) = state.jobs.lock() {
                jobs.dismiss(&job_id)
            } else { false };
            Ok(dismissed)
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    job_table.set("dismiss", dismiss_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clear_finished(ctx: &ApiCtx, lua: &Lua, job_table: &Table) -> Result<()> {
    // clear_finished() → string[]   (ids of dismissed jobs)
    // Drops every terminal-state job in one pass. Useful for "clear all"
    // affordances in monitor-style panels.
    let handle = ctx.app_handle.clone();
    let clear_fn = lua
        .create_function(move |lua_ctx, ()| {
            let Some(ref h) = handle else { return Ok(lua_ctx.create_table()?); };
            let state = h.state::<crate::AppState>();
            let cleared: Vec<String> = if let Ok(mut jobs) = state.jobs.lock() {
                jobs.clear_finished()
            } else { Vec::new() };
            let out = lua_ctx.create_table()?;
            for id in cleared { out.push(id)?; }
            Ok(out)
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    job_table.set("clear_finished", clear_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
