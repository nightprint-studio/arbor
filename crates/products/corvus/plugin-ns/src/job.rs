//! `arbor.job` (background job spawning + introspection), ported to run through
//! an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/job.rs`: same namespace (`arbor.job`), same function names
//! (`spawn` / `list` / `cancel` / `dismiss` / `clear_finished`), same argument
//! shapes, same return conventions, same validation `RuntimeError` strings, same
//! `job.<op> …: …` error prefixes.
//!
//! This is a **PROXY** namespace: the `JobRegistry` (and the OS process the job
//! drives) lives in the shell's `AppState` (`jobs`), not in `corvus-be`. So
//! every op round-trips through the captured `Arc<dyn NsHost>` whose `corvus-be`
//! impl calls back over the reverse channel (`host_call("__job_<op>", …)`); the
//! matching shell handlers in `src-tauri/src/ipc/mod.rs` read/mutate the real
//! `AppState` registry and (for `spawn`) drive the real `crate::jobs::spawn_job`
//! exactly as `ns_shell/job.rs` did. The registry is **not** repo-scoped — it is
//! a single global, so none of these read `__arbor_current_repo__`.
//!
//! Calling convention (unchanged from the shell — see `ns_shell/job.rs`):
//!   · `spawn(config)` is a table-config returning `(job_id, nil) | (nil, err)`.
//!     Validation problems (missing `command`, reserved `system` category) RAISE
//!     installer-side, byte-for-byte with the shell; mutex / spawn failures come
//!     back as the `(nil, err)` tuple.
//!   · `list()` returns `(jobs_array, nil) | (nil, err)`.
//!   · `cancel(job_id)` returns `nil` (best-effort, never fails).
//!   · `dismiss(job_id)` returns `bool` (true if removed; false if running /
//!     unknown).
//!   · `clear_finished()` returns `string[]` (ids of dismissed jobs).
//!
//! The `spawn` config table mirrors the shell exactly:
//! `{ name?, command, cwd?, env?, category?, on_done_action?, on_done?, hidden?,
//! target? }`. The `on_done` Lua callback is registered into **this** host's
//! `__arbor_hooks__` registry under a synthetic action name (`__job_done_<id>__`)
//! and that synthetic name is forwarded as the effective `on_done_action`, so the
//! plugin's closure is what runs when the job finishes — same as the shell.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.job.*` installer. Holds the host handle the closures call through.
pub struct JobInstaller {
    host: NsHostHandle,
}

impl JobInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for JobInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let job_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_spawn(self.host.clone(), ctx, lua, &job_table)?;
        install_list(self.host.clone(), ctx, lua, &job_table)?;
        install_cancel(self.host.clone(), ctx, lua, &job_table)?;
        install_dismiss(self.host.clone(), ctx, lua, &job_table)?;
        install_clear_finished(self.host.clone(), ctx, lua, &job_table)?;

        arbor
            .set("job", job_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_spawn(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    job_table: &Table,
) -> PluginCoreResult<()> {
    // spawn(config) → (job_id, nil) | (nil, err)
    // config: { name, command, cwd?, env?, category?, on_done_action?, on_done?,
    //           hidden?, target? }
    // Validation problems (missing `command`, reserved category) raise.
    // Mutex / spawn failures come back as the (nil, err) tuple.
    let pname = ctx.plugin_name.clone();
    let spawn_fn = lua
        .create_function(move |lua_ctx, config: mlua::Table| -> LuaTuple {
            let name = config
                .get::<String>("name")
                .unwrap_or_else(|_| "Job".to_string());
            let command = config.get::<String>("command").map_err(|_| {
                mlua::Error::RuntimeError("arbor.job.spawn: 'command' is required".to_string())
            })?;
            let cwd: Option<String> = config.get::<Option<String>>("cwd").unwrap_or(None);
            let category: Option<String> = {
                let raw = config.get::<Option<String>>("category").unwrap_or(None);
                match raw {
                    Some(c) => {
                        let trimmed = c.trim().to_string();
                        if trimmed.eq_ignore_ascii_case("system") {
                            return Err(mlua::Error::RuntimeError(
                                "arbor.job.spawn: category 'system' is reserved".to_string(),
                            ));
                        }
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed)
                        }
                    }
                    None => None,
                }
            };
            let on_done_action: Option<String> =
                config.get::<Option<String>>("on_done_action").unwrap_or(None);
            let hidden: bool = config
                .get::<Option<bool>>("hidden")
                .unwrap_or(None)
                .unwrap_or(false);
            // Optional window-routing target. Absent → main window (the only
            // host that also renders untagged jobs).
            let target: Option<String> = config.get::<Option<String>>("target").unwrap_or(None);

            let env: Vec<(String, String)> = config
                .get::<Option<mlua::Table>>("env")
                .unwrap_or(None)
                .map(|t| {
                    let mut pairs = Vec::new();
                    for (k, v) in t.pairs::<String, String>().flatten() {
                        pairs.push((k, v));
                    }
                    pairs
                })
                .unwrap_or_default();

            // Reserve the job id from the shell registry first (the shell owns
            // `JobRegistry::new_id`), so the synthetic on_done hook name and the
            // `arbor://job-started` payload carry the real id, exactly as the
            // shell did inline under its `jobs.lock()`.
            let job_id = match host.job_new_id(
                &name,
                &pname,
                &command,
                category.as_deref(),
                hidden,
                target.as_deref(),
            ) {
                Ok(id) => id,
                Err(e) => return err2(lua_ctx, format!("job.spawn jobs lock: {e}")),
            };

            // Register the optional `on_done` Lua closure into THIS host's hook
            // registry under a synthetic action name and forward that name as the
            // effective on_done_action — byte-for-byte with the shell. The shell
            // job thread fires the action; for a BE-spawned job the shell routes
            // the action back to this BE's plugin host so the closure runs here.
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

            // Convert env pairs into a JSON object for the wire.
            let env_json: serde_json::Map<String, serde_json::Value> = env
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();

            // Drive the real spawn shell-side (registry already holds the job,
            // the shell emits `arbor://job-started` and calls `spawn_job`).
            if let Err(e) = host.job_spawn(serde_json::json!({
                "job_id":         job_id,
                "name":           name,
                "plugin_name":    pname,
                "command":        command,
                "cwd":            cwd,
                "env":            env_json,
                "category":       category,
                "on_done_action": effective_on_done,
                "hidden":         hidden,
                "target":         target,
            })) {
                return err2(lua_ctx, e);
            }

            ok2(lua_ctx, job_id)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    job_table
        .set("spawn", spawn_fn)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    job_table: &Table,
) -> PluginCoreResult<()> {
    let list_fn = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            // Host returns the serde-serialized job list as a JSON array.
            let json = match host.job_list() {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("job.list to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    job_table
        .set("list", list_fn)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    job_table: &Table,
) -> PluginCoreResult<()> {
    // cancel(job_id) → nil   (best-effort, never fails)
    let cancel_fn = lua
        .create_function(move |_lua_ctx, job_id: String| {
            // Best-effort: the shell's `cancel` never fails; swallow any
            // host-call error so the Lua surface stays `→ nil` unconditionally.
            let _ = host.job_cancel(&job_id);
            Ok(())
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    job_table
        .set("cancel", cancel_fn)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_dismiss(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    job_table: &Table,
) -> PluginCoreResult<()> {
    // dismiss(job_id) → bool   (true if removed; false if running / unknown)
    // Mirrors the host's `dismiss_job` Tauri command — only terminal jobs
    // (completed / failed / cancelled) are eligible. Running jobs are ignored so
    // a misclick doesn't leak a process from the registry.
    let dismiss_fn = lua
        .create_function(move |_lua_ctx, job_id: String| {
            // A poisoned-mutex / channel error maps to `false` (not removed),
            // matching the shell's `else { false }` fallback.
            let dismissed = host.job_dismiss(&job_id).unwrap_or(false);
            Ok(dismissed)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    job_table
        .set("dismiss", dismiss_fn)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clear_finished(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    job_table: &Table,
) -> PluginCoreResult<()> {
    // clear_finished() → string[]   (ids of dismissed jobs)
    // Drops every terminal-state job in one pass. Useful for "clear all"
    // affordances in monitor-style panels.
    let clear_fn = lua
        .create_function(move |lua_ctx, ()| {
            // A channel/mutex error maps to an empty list, matching the shell's
            // `else { Vec::new() }` fallback.
            let cleared: Vec<String> = host.job_clear_finished().unwrap_or_default();
            let out = lua_ctx.create_table()?;
            for id in cleared {
                out.push(id)?;
            }
            Ok(out)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    job_table
        .set("clear_finished", clear_fn)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
