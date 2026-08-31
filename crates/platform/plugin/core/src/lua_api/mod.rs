//! `arbor.*` Lua API surface — the host-pure slice plus the wiring that
//! lets the Tauri shell (and, eventually, the domain crates) contribute the
//! namespaces they own through [`LuaNamespaceInstaller`].
//!
//! ## Layout
//!
//! - [`ctx`]      — [`ApiCtx`], the per-`register()` capture bag.
//! - [`helpers`]  — pure helpers shared by every namespace installer
//!   (`fs_perm`, `glob`, `convert`, `tuple`, …).
//! - `ns/`        — *(PR #4 Step 6)* host-pure namespaces.
//!
//! ## How `register()` is wired
//!
//! [`crate::sandbox::create_sandbox`] hands a freshly-built [`mlua::Lua`]
//! and an [`ApiInstallParams`](crate::sandbox::ApiInstallParams) to whichever
//! [`LuaApiInstaller`](crate::sandbox::LuaApiInstaller) the host installed
//! at boot. The Tauri shell's installer (`TauriApiInstaller`) calls
//! [`register`] with the `extra_installers` slice that wraps the
//! ns/* that still live in `src-tauri/src/plugin/api/ns/*`. As ns/*
//! migrate (PR #4 Step 6), the orchestrator below grows hardcoded calls
//! to the in-crate `ns::*::install(...)` and the `extra_installers` slice
//! shrinks to only the namespaces that need shell-side state.

pub mod ctx;
pub mod helpers;
pub(crate) mod ns;

use mlua::{Lua, Table};

use crate::error::{PluginCoreError, Result};
use crate::sandbox::ApiInstallParams;

pub use ctx::ApiCtx;

/// Contract for a Lua namespace installer. One impl per `arbor.<ns>`
/// (or per closely-related cluster). Installers run in the order the host
/// hands them to [`register`]; ordering matters in a few spots:
///   * `events` reads the `__arbor_hooks__` registry that [`register`] sets
///     up before invoking any installer;
///   * `service` bootstraps its own globals;
///   * `ui` and friends rely on the contribution registry being live —
///     which is always is, since it's an Arc clone on the AppState side.
///
/// Implementors should panic-safely produce a [`PluginCoreError`] and clone
/// any captures they need into closures.
pub trait LuaNamespaceInstaller: Send + Sync {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()>;
}

/// Build the `arbor.*` table, run every supplied installer against it, then
/// publish it on the Lua globals.
///
/// `extra_installers` is the host-supplied ordered list of
/// [`LuaNamespaceInstaller`] impls. The Tauri shell currently passes one
/// wrapper per ns/* in `src-tauri/src/plugin/api/ns/*`. As namespaces
/// migrate into plugin-core (Step 6), they move out of the wrapper list
/// and into hardcoded calls inside this function.
pub fn register(
    lua: &Lua,
    params: ApiInstallParams,
    extra_installers: &[std::sync::Arc<dyn LuaNamespaceInstaller>],
) -> Result<()> {
    let ctx = ApiCtx::from_install_params(params);

    let arbor = lua.create_table()
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // Inject hook registry table (must exist before any installer that wires
    // arbor.events.on / arbor.timer / arbor.job can be called).
    let hooks_table = lua.create_table()
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    lua.globals()
        .set("__arbor_hooks__", hooks_table)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    // ── Host-pure namespaces (Step 6) ────────────────────────────────────
    // Hardcoded here in the same relative order they held in the legacy
    // single `register(...)` body. They run before any shell installer so
    // late-arriving shell namespaces (e.g. `ui.branding`) can inspect what
    // a host-pure ns published (e.g. the `arbor.ui` table).
    ns::log::install(&ctx, lua, &arbor)?;
    ns::events::install(&ctx, lua, &arbor)?;
    ns::service::install(&ctx, lua, &arbor)?;
    ns::json::install(&ctx, lua, &arbor)?;
    ns::json_studio::install(&ctx, lua, &arbor)?;
    ns::ron_studio::install(&ctx, lua, &arbor)?;
    ns::toml_studio::install(&ctx, lua, &arbor)?;
    ns::yaml_studio::install(&ctx, lua, &arbor)?;
    ns::properties_studio::install(&ctx, lua, &arbor)?;
    ns::fs::install(&ctx, lua, &arbor)?;
    ns::text::install(&ctx, lua, &arbor)?;
    ns::meta::install(&ctx, lua, &arbor)?;
    ns::settings::install(&ctx, lua, &arbor)?;
    ns::credentials::install(&ctx, lua, &arbor)?;
    // After `credentials`: the same store, reached through a flow instead of by hand.
    ns::oauth::install(&ctx, lua, &arbor)?;
    ns::ext::install(&ctx, lua, &arbor)?;
    ns::http::install(&ctx, lua, &arbor)?;
    ns::timer::install(&ctx, lua, &arbor)?;
    ns::scheduler::install(&ctx, lua, &arbor)?;
    ns::ui::install(&ctx, lua, &arbor)?;
    ns::keybinding::install(&ctx, lua, &arbor)?;
    ns::command::install(&ctx, lua, &arbor)?;
    ns::hooks::install(&ctx, lua, &arbor)?;
    ns::contribution::install(&ctx, lua, &arbor)?;
    ns::notify::install(&ctx, lua, &arbor)?;

    // ── Host-shell installers (always last so they may inspect anything
    //    a host-pure ns has already set up). Order is the caller's
    //    responsibility — the Tauri shell preserves the legacy order.
    for installer in extra_installers {
        installer.install(&ctx, lua, &arbor)?;
    }

    // Publish arbor global.
    lua.globals()
        .set("arbor", arbor)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    Ok(())
}
