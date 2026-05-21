//! `arbor.*` Lua API surface.
//!
//! `register()` is the single public entry point. Internally it builds an
//! [`ApiCtx`] from the per-plugin parameters, then walks the namespace
//! installers in `ns/*`. Each installer attaches one `arbor.<thing>` table
//! and is responsible for its own closure captures.
//!
//! Namespace files keep individual ops as small private `install_*`
//! functions so the file structure mirrors the Lua surface 1:1.

mod ctx;
mod helpers;
mod ns;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use mlua::Lua;

use crate::error::{AppError, Result};
use crate::plugin::contribution::ContributionRegistry;
use crate::plugin::runtime::{
    PluginPermissions, ScheduleRegistry, TimerCancels, TimerCounter,
};
use crate::plugin::tree::{IconRegistry, TreeStore};

use ctx::ApiCtx;

#[allow(clippy::too_many_arguments)]
pub fn register(
    lua: &Lua,
    plugin_name: String,
    plugin_dir: PathBuf,
    arbor_api: u32,
    app_handle: Option<tauri::AppHandle>,
    timer_cancels: TimerCancels,
    timer_counter: TimerCounter,
    schedules:         ScheduleRegistry,
    scheduler_enabled: bool,
    // Permission snapshot captured at load time — never re-read from Lua globals.
    permissions:       PluginPermissions,
    contributions:     ContributionRegistry,
    tree_store:        TreeStore,
    icon_registry:     IconRegistry,
    // Live enable flag — closures that may keep firing after `disable_plugin`
    // (timers, schedulers, async callbacks) consult this to no-op cleanly.
    enabled:           Arc<AtomicBool>,
) -> Result<()> {
    let ctx = ApiCtx::from_register_args(
        plugin_name,
        plugin_dir,
        arbor_api,
        app_handle,
        timer_cancels,
        timer_counter,
        schedules,
        scheduler_enabled,
        permissions,
        contributions,
        tree_store,
        icon_registry,
        enabled,
    );

    let arbor = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    // Inject hook registry table (must exist before arbor.events.on is called).
    let hooks_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    lua.globals()
        .set("__arbor_hooks__", hooks_table)
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    // Order matters in a few spots: `events` reads the hooks table set
    // above; `service` bootstraps its own globals; `ui` and friends rely
    // on the contribution registry being live (always is — it's a clone
    // of an Arc on the AppState side).
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
    ns::repo::install(&ctx, lua, &arbor)?;
    ns::workspace::install(&ctx, lua, &arbor)?;
    ns::tabs::install(&ctx, lua, &arbor)?;
    ns::linked_worktrees::install(&ctx, lua, &arbor)?;
    ns::meta::install(&ctx, lua, &arbor)?;
    ns::settings::install(&ctx, lua, &arbor)?;
    ns::toolchain::install(&ctx, lua, &arbor)?;
    ns::terminal::install(&ctx, lua, &arbor)?;
    ns::job::install(&ctx, lua, &arbor)?;
    ns::http::install(&ctx, lua, &arbor)?;
    ns::notes::install(&ctx, lua, &arbor)?;
    ns::issues::install(&ctx, lua, &arbor)?;
    ns::timer::install(&ctx, lua, &arbor)?;
    ns::scheduler::install(&ctx, lua, &arbor)?;
    ns::ui::install(&ctx, lua, &arbor)?;
    ns::keybinding::install(&ctx, lua, &arbor)?;
    ns::command::install(&ctx, lua, &arbor)?;
    ns::hooks::install(&ctx, lua, &arbor)?;
    ns::contribution::install(&ctx, lua, &arbor)?;
    ns::notify::install(&ctx, lua, &arbor)?;
    ns::pipeline::install(&ctx, lua, &arbor)?;
    ns::mr::install(&ctx, lua, &arbor)?;
    ns::ci::install(&ctx, lua, &arbor)?;
    ns::security::install(&ctx, lua, &arbor)?;
    ns::cloud::install(&ctx, lua, &arbor)?;
    ns::brp::install(&ctx, lua, &arbor)?;

    // Publish arbor global.
    lua.globals()
        .set("arbor", arbor)
        .map_err(|e| AppError::Plugin(e.to_string()))?;

    Ok(())
}
