use std::path::PathBuf;
use mlua::{Lua, LuaOptions, StdLib};

use crate::error::{AppError, Result};
use crate::plugin::contribution::ContributionRegistry;
use crate::plugin::runtime::{
    EnvReadPerm, PluginManifest, ScheduleRegistry, TimerCancels, TimerCounter,
};
use crate::plugin::tree::{IconRegistry, TreeStore};

// Embedded built-in Lua utility modules. Injected as require("arbor.*") preloads.
const SCHEMA_LUA:    &str = include_str!("lua_builtins/schema.lua");
const ASYNC_LUA:     &str = include_str!("lua_builtins/async_lib.lua");
const EVENT_LUA:     &str = include_str!("lua_builtins/event.lua");

// Promise bridge — wraps the Rust-backed async APIs (service.call / job.spawn /
// ui.confirm) so they return arbor.async.Promise. Loaded as a one-shot script
// after the arbor.* global is published, NOT via package.preload.
const PROMISE_BRIDGE_LUA: &str = include_str!("lua_builtins/promise_bridge.lua");

// Builder DSL — chainable sugar over arbor.pipeline.define / arbor.ui.form.
// Loaded as a one-shot script after arbor.* is published; installs metatables
// on arbor.pipeline and arbor.ui.form that intercept __call only, leaving the
// table-config entry points untouched.
const BUILDERS_LUA: &str = include_str!("lua_builtins/builders.lua");

// arbor.core.* — pipeline op catalog (structured edit / assertion).
// Opt-in: plugins `require("arbor.core.<topic>")` and call `.register()` to
// expose the ops under their bare names in the pipeline registry. `_util` is
// internal — it's preloaded so the public modules can require it, but plugins
// shouldn't. Bare-fs and bare-text ops aren't shipped here: they're trivial
// `arbor.fs` / `arbor.text` wrappers, so plugins inline them when needed.
const CORE_UTIL_LUA:    &str = include_str!("lua_builtins/core/_util.lua");
const CORE_EDIT_LUA:    &str = include_str!("lua_builtins/core/edit.lua");
const CORE_ASSERT_LUA:  &str = include_str!("lua_builtins/core/assert.lua");

/// Create a sandboxed Lua runtime for a plugin.
pub fn create_sandbox(
    manifest:      &PluginManifest,
    app_handle:    Option<tauri::AppHandle>,
    timer_cancels: TimerCancels,
    timer_counter: TimerCounter,
    schedules:     ScheduleRegistry,
    contributions: ContributionRegistry,
    tree_store:    TreeStore,
    icon_registry: IconRegistry,
    // Live enable flag — captured by long-lived closures (e.g. arbor.log.*)
    // so they can short-circuit when the plugin is disabled mid-call.
    enabled:       std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<Lua> {
    // Load standard libraries. IO is never loaded — plugins use arbor.fs.
    // UTF8 is included so plugins can build glyph tables (utf8.char) and walk
    // multi-byte strings (utf8.codes / utf8.len) without re-implementing them.
    let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::OS | StdLib::PACKAGE | StdLib::UTF8;

    let lua = Lua::new_with(libs, LuaOptions::default())
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    // Stash plugin name + AppHandle in Lua app_data so code paths that only
    // see `&Lua` (hook dispatch, service callbacks) can surface runtime
    // errors to the Plugin Logs panel without extra plumbing.
    crate::plugin::lua_ctx::install(&lua, manifest.name.clone(), app_handle.clone());

    // ── Register the arbor.* API ──────────────────────────────────────────────
    crate::plugin::api::register(
        &lua,
        manifest.name.clone(),
        manifest.dir.clone(),
        manifest.arbor_api,
        app_handle,
        timer_cancels,
        timer_counter,
        schedules,
        manifest.scheduler.enabled,
        // Permissions snapshot — captured at load time, never re-read from Lua.
        manifest.permissions.clone(),
        contributions,
        tree_store,
        icon_registry,
        enabled,
    )?;

    // ── Override print() to route through tracing ─────────────────────────────
    {
        let pname = manifest.name.clone();
        let print_fn = lua.create_function(move |_, args: mlua::Variadic<mlua::Value>| {
            let parts: Vec<String> = args.iter().map(|v| match v {
                mlua::Value::String(s)  => s.to_str().map(|it| it.to_string()).unwrap_or("?".to_string()),
                mlua::Value::Integer(i) => i.to_string(),
                mlua::Value::Number(n)  => n.to_string(),
                mlua::Value::Boolean(b) => b.to_string(),
                mlua::Value::Nil        => "nil".to_string(),
                _                       => "[?]".to_string(),
            }).collect();
            tracing::info!(target: "plugin", "[{pname}] {}", parts.join("\t"));
            Ok(())
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        lua.globals()
            .set("print", print_fn)
            .map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // ── Harden the OS table ───────────────────────────────────────────────────
    harden_os_table(&lua, manifest)?;

    // ── package.preload for arbor.* sub-namespaces ────────────────────────────
    // Allows `require("arbor.log")`, `require("arbor.fs")`, etc.
    lua.load(ARBOR_PRELOAD_LUA).exec()
        .map_err(|e| AppError::Plugin(format!("failed to set up preloads: {e}")))?;

    // ── Inject utility modules as preloads ────────────────────────────────────
    inject_builtin_module(&lua, "arbor.schema", SCHEMA_LUA)?;
    inject_builtin_module(&lua, "arbor.async",  ASYNC_LUA)?;
    inject_builtin_module(&lua, "arbor.event",  EVENT_LUA)?;

    // arbor.core.* — opt-in pipeline op catalog. Every plugin can `require`
    // any of these; they do NOT auto-register anything until the plugin
    // calls `.register()` on the module it wants exposed. `_util` is
    // preloaded (siblings need it) but is internal — the leading `_`
    // signals "not for plugin code".
    inject_builtin_module(&lua, "arbor.core._util",   CORE_UTIL_LUA)?;
    inject_builtin_module(&lua, "arbor.core.edit",    CORE_EDIT_LUA)?;
    inject_builtin_module(&lua, "arbor.core.assert",  CORE_ASSERT_LUA)?;

    // ── Sandboxed require() — restrict to plugin directory only ───────────────
    setup_require_sandbox(&lua, &manifest.dir)?;

    // ── Promise bridge — wraps service.call / job.spawn / ui.confirm ──────────
    // Must run AFTER arbor.* is published (api::register sets the global) and
    // AFTER arbor.async is preloaded so the bridge can `require` it.
    lua.load(PROMISE_BRIDGE_LUA)
        .set_name("arbor:promise_bridge")
        .exec()
        .map_err(|e| AppError::Plugin(format!("promise bridge: {e}")))?;

    // ── Builder DSL — installs __call metamethods on arbor.pipeline and
    // arbor.ui.form. Order vs the promise bridge is irrelevant; both run after
    // arbor.* is published.
    lua.load(BUILDERS_LUA)
        .set_name("arbor:builders")
        .exec()
        .map_err(|e| AppError::Plugin(format!("builders: {e}")))?;

    Ok(lua)
}

// ---------------------------------------------------------------------------
// OS hardening — remove dangerous functions, respect env_read permission
// ---------------------------------------------------------------------------

fn harden_os_table(lua: &Lua, manifest: &PluginManifest) -> Result<()> {
    let globals = lua.globals();
    let os: mlua::Table = globals.get("os")
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    // Always remove these dangerous functions.
    for func in &["execute", "exit", "remove", "rename", "tmpname"] {
        os.set(*func, mlua::Value::Nil)
            .map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // env_read: gate os.getenv based on the configured policy.
    match &manifest.permissions.env_read {
        EnvReadPerm::All(true) => {
            // Native getenv — readable.
        }
        EnvReadPerm::All(false) => {
            os.set("getenv", mlua::Value::Nil)
                .map_err(|e| AppError::Plugin(e.to_string()))?;
        }
        EnvReadPerm::Allowlist(list) => {
            let allowed: std::collections::HashSet<String> = list.iter().cloned().collect();
            let getenv = lua
                .create_function(move |_, name: String| {
                    if allowed.contains(&name) {
                        Ok(std::env::var(&name).ok())
                    } else {
                        Ok(None)
                    }
                })
                .map_err(|e| AppError::Plugin(e.to_string()))?;
            os.set("getenv", getenv)
                .map_err(|e| AppError::Plugin(e.to_string()))?;
        }
    }

    // Remove io table entirely — plugins use arbor.fs.
    globals.set("io", mlua::Value::Nil)
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// require() sandbox — restrict to the plugin's own directory
// ---------------------------------------------------------------------------

fn setup_require_sandbox(lua: &Lua, plugin_dir: &PathBuf) -> Result<()> {
    let dir = plugin_dir.clone();

    // Build the custom searcher as a Rust function.
    let sandbox_searcher = lua
        .create_function(move |lua_ctx, modname: String| {
            // "ui.forms" → "ui/forms.lua" (OS separator)
            let sep  = std::path::MAIN_SEPARATOR_STR;
            let rel  = modname.replace('.', sep);
            let candidate = dir.join(format!("{rel}.lua"));

            // Verify path is inside the plugin directory (path-traversal guard).
            let canon_dir = match std::fs::canonicalize(&dir) {
                Ok(p)  => p,
                Err(_) => return Ok(mlua::MultiValue::from_vec(vec![
                    mlua::Value::String(lua_ctx.create_string(
                        format!("\tcannot resolve plugin dir: {}", dir.display()).as_bytes()
                    )?)
                ])),
            };
            let canon_file = match std::fs::canonicalize(&candidate) {
                Ok(p)  => p,
                Err(_) => return Ok(mlua::MultiValue::from_vec(vec![
                    mlua::Value::String(lua_ctx.create_string(
                        format!("\tno file '{rel}.lua' in plugin dir").as_bytes()
                    )?)
                ])),
            };
            if !canon_file.starts_with(&canon_dir) {
                return Err(mlua::Error::RuntimeError(format!(
                    "require '{}': path traversal detected", modname
                )));
            }

            let code = match std::fs::read_to_string(&canon_file) {
                Ok(c)  => c,
                Err(e) => return Err(mlua::Error::RuntimeError(format!(
                    "require '{}': {e}", modname
                ))),
            };

            let loader = lua_ctx.load(code).set_name(modname).into_function()?;
            Ok(mlua::MultiValue::from_vec(vec![mlua::Value::Function(loader)]))
        })
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    // Replace package.searchers: keep only [1] (preload) then our sandbox loader.
    lua.load(r#"
        local old = package.searchers or package.loaders
        package.searchers = { old[1] }
        package.path  = ""
        package.cpath = ""
    "#).exec().map_err(|e| AppError::Plugin(e.to_string()))?;

    let package: mlua::Table = lua.globals().get("package")
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    let searchers: mlua::Table = package.get("searchers")
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    searchers.push(sandbox_searcher)
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Inject a built-in Lua module into package.preload
// ---------------------------------------------------------------------------

fn inject_builtin_module(lua: &Lua, name: &str, source: &str) -> Result<()> {
    let loader = lua.load(source)
        .set_name(name)
        .into_function()
        .map_err(|e| AppError::Plugin(format!("builtin module '{name}': {e}")))?;

    let package: mlua::Table = lua.globals().get("package")
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    let preload: mlua::Table = package.get("preload")
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    preload.set(name, loader)
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Package preloads for arbor.* sub-namespaces
// ---------------------------------------------------------------------------

const ARBOR_PRELOAD_LUA: &str = r#"
local a = arbor
package.preload["arbor"]            = function() return a end
package.preload["arbor.events"]     = function() return a.events end
package.preload["arbor.log"]        = function() return a.log end
package.preload["arbor.json"]       = function() return a.json end
package.preload["arbor.fs"]         = function() return a.fs end
package.preload["arbor.repo"]       = function() return a.repo end
package.preload["arbor.meta"]       = function() return a.meta end
package.preload["arbor.timer"]      = function() return a.timer end
package.preload["arbor.ui"]         = function() return a.ui end
package.preload["arbor.job"]        = function() return a.job end
package.preload["arbor.terminal"]   = function() return a.terminal end
package.preload["arbor.settings"]   = function() return a.settings end
package.preload["arbor.keybinding"] = function() return a.keybinding end
package.preload["arbor.pipeline"]   = function() return a.pipeline end
package.preload["arbor.command"]    = function() return a.command end
package.preload["arbor.issues"]     = function() return a.issues end
package.preload["arbor.service"]    = function() return a.service end
package.preload["arbor.workspace"]  = function() return a.workspace end
package.preload["arbor.hooks"]      = function() return a.hooks end
"#;
