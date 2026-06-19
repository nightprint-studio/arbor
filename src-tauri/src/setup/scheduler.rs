//! Shared trigger-engine (`arbor-scheduler`) wiring + PluginHost host-context
//! install. Built on the tauri-managed Tokio runtime and wired into `AppState`
//! and `PluginHost` BEFORE the marketplace auto-refresh or the plugin boot
//! thread tries to register against it.

use std::sync::Arc;

use arbor_scheduler::prelude::Scheduler;
use tauri::Manager;

use crate::AppState;

/// Build the scheduler + hand the host context / api installer / extra plugin
/// roots to `PluginHost`.
pub fn wire(app: &tauri::App) {
    let state = app.state::<AppState>();

    // Tauri's `async_runtime::spawn` is usable from sync `setup()` and runs the
    // future on its internal Tokio runtime — capture `Handle::current()` from
    // inside that future to get the runtime handle the scheduler needs.
    let (tx, rx) = std::sync::mpsc::sync_channel::<tokio::runtime::Handle>(1);
    tauri::async_runtime::spawn(async move {
        let _ = tx.send(tokio::runtime::Handle::current());
    });
    let rt_handle = rx
        .recv()
        .expect("could not capture tokio runtime handle for arbor-scheduler");

    let ctx: Arc<dyn arbor_core::prelude::AppCtx> = Arc::new(crate::app_ctx::TauriAppCtx::new(
        app.handle().clone(),
        state.app_focused.clone(),
    ));

    // Hand the host context + the Lua API installer to PluginHost. `set_app_ctx`
    // also routes the AppCtx into the ContributionRegistry so the coalesced
    // `arbor://contributions-changed` / `arbor://containers-changed` emits stay
    // routed to the frontend (PR #4 — `arbor-plugin-core` migration).
    {
        let mut host = state
            .plugin_host
            .lock()
            .expect("plugin_host poisoned at AppCtx install");
        host.set_app_ctx(ctx.clone());
        host.set_api_installer(crate::plugin::api_installer::tauri_api_installer());
        // Marketplace install dir is scanned alongside the host's dev
        // `plugin_dir()` during reload. Passed as an extra root so
        // `arbor-plugin-core` itself stays free of any marketplace coupling.
        host.set_extra_plugin_roots(vec![arbor_plugin_marketplace::prelude::plugins_dir()]);
    }

    let scheduler = Arc::new(Scheduler::new(ctx, rt_handle));
    let _ = state.scheduler.set(scheduler.clone());

    // Hand the scheduler + a weak self-pointer to PluginHost so Lua-fired
    // actions can call back into `hook_router::fire_on`.
    let host_arc = state.plugin_host.clone();
    {
        let mut host = host_arc
            .lock()
            .expect("plugin_host poisoned during scheduler install");
        host.install_scheduler(scheduler, Arc::downgrade(&host_arc));
    }
}
