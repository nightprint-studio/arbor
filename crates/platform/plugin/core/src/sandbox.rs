//! Per-plugin Lua VM construction.
//!
//! The sandbox is a stripped-down [`mlua::Lua`] with:
//!   * a curated set of standard libraries loaded (no `io`),
//!   * `os.*` hardened down to what the plugin's `permissions.env_read`
//!     policy allows,
//!   * `require()` restricted to files inside the plugin's own directory,
//!   * the [embedded Lua builtins](lua_builtins/) (`arbor.schema`,
//!     `arbor.async`, `arbor.event`, `arbor.core.*`) injected as
//!     `package.preload` entries,
//!   * a wrapper installed over `print` that routes through `tracing`.
//!
//! Construction of the `arbor.*` namespace is deferred to a caller-supplied
//! [`LuaApiInstaller`] — the runtime-side API surface lives in another
//! module (`lua_api`, atterrato in sessione 4 di PR #4) or, until that
//! module migrates, in the host shell crate via a shim installer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::AtomicBool;

use arbor_core::prelude::AppCtx;
use arbor_plugin_types::prelude::{EnvReadPerm, Manifest, Permissions, ScheduleRegistry};
use mlua::{Lua, LuaOptions, StdLib};

use crate::contribution::ContributionRegistry;
use crate::error::{PluginCoreError, Result};
use crate::runtime::host::PluginHost;
use crate::runtime::loaded::{PluginActivity, ServiceIndex, TimerCancels, TimerCounter};
use crate::tree::{IconRegistry, TreeStore};

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

// ---------------------------------------------------------------------------
// LuaApiInstaller — host-shell-supplied installer that publishes the
// `arbor.*` namespace into a freshly-built sandbox VM.
//
// The implementation lives wherever the runtime API surface lives. In the
// pre-step-5 codebase that's `src-tauri/src/plugin/api/mod.rs::register`;
// after step 5 it migrates into `arbor-plugin-core::lua_api`. Either way,
// `arbor-plugin-core::sandbox::create_sandbox` only sees the trait.
// ---------------------------------------------------------------------------

/// The long-lived registries a sandbox shares with the runtime that owns it.
///
/// All six are handles — `Arc` inside — so this whole struct is cheap to clone and cloning
/// it shares the registry rather than copying it. They travel together because they have
/// the same lifetime and the same reason to exist: a plugin's Lua closures write into them
/// and the host reads them back out, long after `create_sandbox` has returned.
///
/// Grouped rather than passed flat because six of [`create_sandbox`]'s thirteen parameters
/// were these, all of the same shape, in an order nothing but the compiler was checking —
/// and two of them (`timer_cancels`, `timer_counter`) are a pair whose swap type-checks.
#[derive(Clone)]
pub struct SandboxRegistries {
    /// Cancellation handles for `arbor.timer.after` / `.every`, keyed by timer id.
    pub timer_cancels: TimerCancels,
    /// Monotonic source of those ids.
    pub timer_counter: TimerCounter,
    /// Schedules declared through `arbor.scheduler`.
    pub schedules: ScheduleRegistry,
    /// What the plugin contributes to host surfaces (panels, menus, actions).
    pub contributions: ContributionRegistry,
    /// Tree models built through `arbor.ui.tree`.
    pub tree_store: TreeStore,
    /// Icons the plugin registered, resolvable by name from any of its nodes.
    pub icon_registry: IconRegistry,
    /// Which sibling plugins are live, readable without the host mutex — see
    /// [`PluginActivity`] for why that distinction is load-bearing.
    pub activity: PluginActivity,
    /// Which cross-plugin services exist, on the same terms.
    pub services: ServiceIndex,
}

/// Bag of parameters the [`LuaApiInstaller`] needs to wire the `arbor.*`
/// namespace into a fresh sandbox. Packaged as a struct so adding new
/// fields doesn't churn every call site.
pub struct ApiInstallParams {
    pub plugin_name:        String,
    pub plugin_dir:         PathBuf,
    pub arbor_api:          u32,
    /// Id of the product whose host is loading this plugin (`"corvus"`,
    /// `"garrulus"`, `"launcher"`, …) — the namespace an unqualified
    /// `arbor.events.on("commit", …)` resolves against (**D9**). `None` in
    /// headless / test runs whose host never called `set_product`, in which
    /// case subscriptions are taken exactly as written.
    pub product:            Option<String>,
    pub app_ctx:            Option<Arc<dyn AppCtx>>,
    /// Weak self-reference of the owning [`PluginHost`]. Captured by the
    /// `arbor.*` namespaces that need to fire hooks / invoke services back
    /// into the runtime from a background thread (`events`, `timer`,
    /// `service`, `http`, …). `None` in headless / test runs that don't go
    /// through a `PluginHost` (e.g. the standalone `load_plugin` helper
    /// called from a unit test).
    pub host_weak:          Option<Weak<Mutex<PluginHost>>>,
    /// The shared registries, intact from the caller — see [`SandboxRegistries`].
    pub registries:         SandboxRegistries,
    pub scheduler_enabled:  bool,
    pub permissions:        Permissions,
    /// Keys of the `[[credentials]]` slots the manifest declared. A snapshot, like the
    /// permissions beside it: the set a plugin may reach is fixed when it loads, so an edit
    /// to `plugin.toml` cannot widen a running plugin's reach.
    pub credential_slots:   Vec<String>,
    pub enabled:            Arc<AtomicBool>,
}

/// Publishes the `arbor.*` Lua namespace into a sandbox VM. One implementation
/// per host shell — production wires the Tauri-aware installer; tests can
/// install a no-op stub.
pub trait LuaApiInstaller: Send + Sync {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> Result<()>;
}

/// No-op installer — useful for tests / headless runs where the `arbor.*`
/// namespace isn't needed (no plugin actually calls into it).
pub struct NoopApiInstaller;

impl LuaApiInstaller for NoopApiInstaller {
    fn install(&self, _lua: &Lua, _params: ApiInstallParams) -> Result<()> {
        Ok(())
    }
}

/// Create a sandboxed Lua runtime for a plugin.
pub fn create_sandbox(
    manifest:      &Manifest,
    // Product id of the loading host — see `ApiInstallParams::product`.
    product:       Option<String>,
    app_ctx:       Option<Arc<dyn AppCtx>>,
    host_weak:     Option<Weak<Mutex<PluginHost>>>,
    api_installer: &dyn LuaApiInstaller,
    registries:    SandboxRegistries,
    // Live enable flag — captured by long-lived closures (e.g. arbor.log.*)
    // so they can short-circuit when the plugin is disabled mid-call.
    enabled:       Arc<AtomicBool>,
) -> Result<Lua> {
    // Load standard libraries. IO is never loaded — plugins use arbor.fs.
    // UTF8 is included so plugins can build glyph tables (utf8.char) and walk
    // multi-byte strings (utf8.codes / utf8.len) without re-implementing them.
    let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::OS | StdLib::PACKAGE | StdLib::UTF8;

    let lua = Lua::new_with(libs, LuaOptions::default())
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // Stash plugin name + AppCtx handle in Lua app_data so code paths that
    // only see `&Lua` (hook dispatch, service callbacks) can surface runtime
    // errors to the Plugin Logs panel without extra plumbing.
    crate::lua_ctx::install(&lua, manifest.name.clone(), app_ctx.clone());

    // ── Register the arbor.* API ──────────────────────────────────────────────
    api_installer.install(&lua, ApiInstallParams {
        plugin_name:        manifest.name.clone(),
        plugin_dir:         manifest.dir.clone(),
        arbor_api:          manifest.arbor_api,
        product,
        app_ctx,
        host_weak,
        registries,
        scheduler_enabled:  manifest.scheduler.enabled,
        // Permissions snapshot — captured at load time, never re-read from Lua.
        permissions:        manifest.permissions.clone(),
        credential_slots:   manifest.credentials.iter().map(|c| c.key.clone()).collect(),
        enabled,
    })?;

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
        }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        lua.globals()
            .set("print", print_fn)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // ── Harden the OS table ───────────────────────────────────────────────────
    harden_os_table(&lua, manifest)?;

    // ── package.preload for arbor.* sub-namespaces ────────────────────────────
    // Allows `require("arbor.log")`, `require("arbor.fs")`, etc.
    lua.load(ARBOR_PRELOAD_LUA).exec()
        .map_err(|e| PluginCoreError::Plugin(format!("failed to set up preloads: {e}")))?;

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
        .map_err(|e| PluginCoreError::Plugin(format!("promise bridge: {e}")))?;

    // ── Builder DSL — installs __call metamethods on arbor.pipeline and
    // arbor.ui.form. Order vs the promise bridge is irrelevant; both run after
    // arbor.* is published.
    lua.load(BUILDERS_LUA)
        .set_name("arbor:builders")
        .exec()
        .map_err(|e| PluginCoreError::Plugin(format!("builders: {e}")))?;

    Ok(lua)
}

// ---------------------------------------------------------------------------
// OS hardening — remove dangerous functions, respect env_read permission
// ---------------------------------------------------------------------------

fn harden_os_table(lua: &Lua, manifest: &Manifest) -> Result<()> {
    let globals = lua.globals();
    let os: mlua::Table = globals.get("os")
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // Always remove these dangerous functions.
    for func in &["execute", "exit", "remove", "rename", "tmpname"] {
        os.set(*func, mlua::Value::Nil)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // env_read: gate os.getenv based on the configured policy.
    match &manifest.permissions.env_read {
        EnvReadPerm::All(true) => {
            // Native getenv — readable.
        }
        EnvReadPerm::All(false) => {
            os.set("getenv", mlua::Value::Nil)
                .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
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
                .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
            os.set("getenv", getenv)
                .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        }
    }

    // Remove io table entirely — plugins use arbor.fs.
    globals.set("io", mlua::Value::Nil)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// require() sandbox — restrict to the plugin's own directory
// ---------------------------------------------------------------------------

fn setup_require_sandbox(lua: &Lua, plugin_dir: &Path) -> Result<()> {
    let dir = plugin_dir.to_path_buf();

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
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // Replace package.searchers: keep only [1] (preload) then our sandbox loader.
    lua.load(r#"
        local old = package.searchers or package.loaders
        package.searchers = { old[1] }
        package.path  = ""
        package.cpath = ""
    "#).exec().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let package: mlua::Table = lua.globals().get("package")
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    let searchers: mlua::Table = package.get("searchers")
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    searchers.push(sandbox_searcher)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Inject a built-in Lua module into package.preload
// ---------------------------------------------------------------------------

fn inject_builtin_module(lua: &Lua, name: &str, source: &str) -> Result<()> {
    let loader = lua.load(source)
        .set_name(name)
        .into_function()
        .map_err(|e| PluginCoreError::Plugin(format!("builtin module '{name}': {e}")))?;

    let package: mlua::Table = lua.globals().get("package")
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    let preload: mlua::Table = package.get("preload")
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    preload.set(name, loader)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::BUILDERS_LUA;
    use mlua::Lua;

    /// Exec the builder chunk against a hand-built `arbor` global.
    ///
    /// The chunk is the last thing `create_sandbox` runs, and it runs for *every* plugin in
    /// *every* product — so what it assumes about the namespace table is a cross-product
    /// invariant. Driving it directly keeps that invariant testable without standing up a
    /// manifest, an `AppCtx` and eight registries.
    fn exec_builders(setup: &str) -> mlua::Result<()> {
        let lua = Lua::new();
        lua.load(setup).exec()?;
        lua.load(BUILDERS_LUA).set_name("arbor:builders").exec()
    }

    /// The shape a shell-hosted product has: `arbor.pipeline` is a callable table.
    const WITH_PIPELINE: &str = r#"
        arbor = {
          pipeline = setmetatable({ define = function(cfg) return cfg end }, {}),
          ui = { form = setmetatable({}, { __call = function(_, cfg) return cfg end }) },
        }
    "#;

    /// The shape a host-pure backend has: no `pipeline` at all — the shell publishes it.
    const WITHOUT_PIPELINE: &str = r#"
        arbor = {
          ui = { form = setmetatable({}, { __call = function(_, cfg) return cfg end }) },
        }
    "#;

    #[test]
    fn the_pipeline_builder_is_installed_when_the_namespace_exists() {
        let lua = Lua::new();
        lua.load(WITH_PIPELINE).exec().unwrap();
        lua.load(BUILDERS_LUA).set_name("arbor:builders").exec().unwrap();

        // `arbor.pipeline("id")` now returns a builder rather than erroring on a
        // non-callable table.
        let id: String = lua
            .load(r#"return arbor.pipeline("deploy"):name("Deploy")._cfg.id"#)
            .eval()
            .unwrap();
        assert_eq!(id, "deploy");
    }

    #[test]
    fn a_host_without_a_pipeline_namespace_still_loads_the_chunk() {
        // Regression: `setmetatable(arbor.pipeline, …)` on a nil raised "bad argument #1 to
        // 'setmetatable'", which aborted sandbox setup — so under bennu/sitta/tyto/garrulus
        // *every* plugin failed to load, all with the same error about a namespace none of
        // them had asked for.
        exec_builders(WITHOUT_PIPELINE).expect("builders must load where the shell does not");
    }

    #[test]
    fn the_form_builder_survives_the_absence_of_a_pipeline() {
        // The two halves of the chunk are independent: losing the pipeline sugar must not
        // cost the form sugar, which every product does have.
        let lua = Lua::new();
        lua.load(WITHOUT_PIPELINE).exec().unwrap();
        lua.load(BUILDERS_LUA).set_name("arbor:builders").exec().unwrap();

        let title: String = lua
            .load(r#"return arbor.ui.form():title("Hi")._cfg.title"#)
            .eval()
            .unwrap();
        assert_eq!(title, "Hi");
    }
}
