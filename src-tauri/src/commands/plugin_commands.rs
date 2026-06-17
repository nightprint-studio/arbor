use std::sync::atomic::Ordering;

use tauri::{Emitter, State};

use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// NOTE: the leaf-clean subset of this domain (plugin discovery/reflection,
// plugin settings file read/write, toolchain registry, contribution +
// container registry exposure) has been migrated to the platform backend —
// see `crate::ipc::platform::plugin`. Everything left here mutates the plugin
// runtime, executes Lua, fires hooks, emits `arbor://*` events, takes an
// `AppHandle`, or is the boot/focus handshake (no `Result`), so it stays as a
// keep-shell Tauri command.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Master plugin-system kill-switch (Plugin Manager toggle)
// ---------------------------------------------------------------------------
//
// The plugin runtime is opt-in: by default a fresh install starts with NO
// plugins loaded. The user explicitly turns the system on through the toggle
// at the top of the Plugin Manager, and that choice is persisted in
// `config.toml::plugins_enabled`. When toggled off the runtime is torn down
// (schedulers cancelled, contributions wiped, plugin list emptied) and at
// startup nothing is even discovered from disk.

#[tauri::command]
pub fn set_plugins_enabled(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), AppError> {
    // Persist the choice immediately so a crash between here and the runtime
    // mutation can't leave the saved state out of sync with what was applied.
    {
        let mut cfg = state.lock_config()?;
        if cfg.plugins_enabled == enabled {
            return Ok(());
        }
        cfg.plugins_enabled = enabled;
        if let Err(e) = crate::config::app_config::save(&cfg) {
            tracing::warn!("failed to persist plugins_enabled: {e}");
        }
    }

    if enabled {
        // Re-discover and load everything from disk. Mirrors the regular
        // reload command, including re-firing `on_repo_open` for open tabs.
        reload_plugins(app_handle, state)?;
    } else {
        // Cancel any running plugin job so background processes don't outlive
        // the runtime that owns them.
        if let Ok(mut jobs) = state.jobs.lock() {
            jobs.cancel_by_plugin(None);
        }
        {
            let mut host = state.lock_plugin_host()?;
            host.unload_all();
        }
        let _ = app_handle.emit("arbor://plugins-reloaded", ());
    }
    Ok(())
}

#[tauri::command]
pub fn reload_plugins(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    // Cancel all running plugin jobs before reloading so stale processes don't linger.
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.cancel_by_plugin(None);
    }
    {
        let mut host = state.lock_plugin_host()?;
        host.reload()?;
        host.start_all_schedulers();
    } // release lock before emitting

    // Re-fire `on_repo_open` for every currently-open tab so plugins that
    // depend on this lifecycle hook (badges, tab-scoped caches, MR fetchers,
    // etc.) can rebuild their per-repo state after a hot reload. Without
    // this the user would have to manually close and reopen every tab to
    // see plugin behaviour resume.
    let opens: Vec<(String, String, String)> = {
        match state.lock_repos() {
            Ok(mgr) => mgr.list_open(),
            Err(_)  => Vec::new(),
        }
    };
    if !opens.is_empty() {
        for (tab_id, path, name) in &opens {
            state.fire_hook("on_repo_open", serde_json::json!({
                "tab_id": tab_id,
                "path":   path,
                "name":   name,
            }));
        }

        // `list_open()` iterates a HashMap — order is non-deterministic,
        // so plugins that derive their `current_repo` from the LAST
        // `on_repo_open` they receive end up pointing at a random tab
        // instead of the one the user is actually looking at. Fire one
        // final `on_tab_switch` for the active tab so plugins land on the
        // right repo AND those that subscribe only to `on_tab_switch`
        // (not `on_repo_open`) wake up too.
        let active_tab = state.active_tab_id.lock().ok().and_then(|g| g.clone());
        if let Some(tid) = active_tab {
            if let Some((tab_id, path, name)) = opens.iter().find(|(t, _, _)| t == &tid) {
                state.fire_hook("on_tab_switch", serde_json::json!({
                    "tab_id": tab_id,
                    "path":   path,
                    "name":   name,
                }));
            }
        }
    }

    let _ = app_handle.emit("arbor://plugins-reloaded", ());
    Ok(())
}

#[tauri::command]
pub fn exec_hook(
    state: State<'_, AppState>,
    hook: String,
    context_json: String,
) -> Result<(), AppError> {
    let ctx: serde_json::Value =
        serde_json::from_str(&context_json).unwrap_or_else(|_| serde_json::json!({}));
    state.fire_hook(&hook, ctx);
    Ok(())
}

/// Fire a specific action on a specific plugin.
/// The frontend calls this when the user interacts with a plugin-registered UI element.
#[tauri::command]
pub fn fire_plugin_action(
    state: State<'_, AppState>,
    plugin_name: String,
    action: String,
    context_json: String,
) -> Result<(), AppError> {
    let host = state.lock_plugin_host()?;
    // Fire the action directly by name — Lua plugins register with arbor.events.on("action-name", fn)
    arbor_plugin_core::prelude::fire_on(&host, &plugin_name, &action, &context_json);
    Ok(())
}

/// Invoke a registered command on behalf of `caller_plugin` — the declarative
/// `kind = "command"` dispatch path. Resolution + the two capability gates
/// (`command_invoke` + the command's `required` tier) live in
/// [`PluginHost::invoke_command`], shared with `arbor.command.fire`.
///
/// `args` is the static argument data declared on the dispatch slot;
/// `context_json` is the node payload (form values + state). They are merged
/// into the ctx delivered to the owner's `command:<id>` handler — node payload
/// fields stay top-level, `args` lands under the `args` key.
#[tauri::command]
pub fn fire_command(
    state: State<'_, AppState>,
    caller_plugin: String,
    id: String,
    args: Option<serde_json::Value>,
    context_json: String,
) -> Result<(), AppError> {
    let mut ctx: serde_json::Value =
        serde_json::from_str(&context_json).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(a) = args {
        if !a.is_null() {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("args".to_string(), a);
            }
        }
    }
    let host = state.lock_plugin_host()?;
    host.invoke_command(&caller_plugin, &id, &ctx)
        .map_err(|e| AppError::Other(format!("{}: {}", e.kind(), e.message())))?;
    Ok(())
}

/// Enable a plugin. Returns the ordered list of plugins that were actually
/// enabled (transitive required deps + target). Returns an error when a
/// required dep is missing or unloadable — call `plugin_enable_preview`
/// first to detect blockers and prompt the user.
#[tauri::command]
pub fn enable_plugin(state: State<'_, AppState>, name: String) -> Result<Vec<String>, AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.enable_plugin(&name)?)
}

/// Uninstall a plugin. Removes the folder under `plugins/`, wipes its
/// global `plugin_data[-dev]/<name>/`, drops its entry from `plugin_states*.json`,
/// and deletes per-repo `.arbor/plugins/<name>/` from every currently open
/// tab and every repo in the workspace registry. Running plugin jobs are
/// cancelled first.
///
/// Returns a list of non-fatal warnings (paths that couldn't be removed) —
/// the in-memory plugin state is always cleared even if some files survive.
#[tauri::command]
pub fn delete_plugin(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<String>, AppError> {
    // Cancel running jobs from this plugin before tearing it down.
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.cancel_by_plugin(Some(&name));
    }

    // Collect every repo path we should clean — open tabs + everything in
    // the workspace registry — before locking the plugin host so we don't
    // hold two managers' mutexes at once.
    let mut repo_paths: Vec<String> = Vec::new();
    if let Ok(mgr) = state.lock_repos() {
        for (_, path, _) in mgr.list_open() { repo_paths.push(path); }
    }
    if let Ok(reg) = state.lock_repo_registry() {
        for entry in reg.list() { repo_paths.push(entry.path); }
    }
    repo_paths.sort();
    repo_paths.dedup();

    let warnings = {
        let mut host = state.lock_plugin_host()?;
        host.delete_plugin(&name, &repo_paths)?
    };

    // Tell the rest of the app to refresh — the Plugin Manager listens for
    // this and reloads its list, contribution registry, etc.
    let _ = app_handle.emit("arbor://plugins-reloaded", ());
    Ok(warnings)
}

/// Disable a plugin. Returns the ordered list of plugins that were actually
/// disabled — `name` plus every transitively-required dependent. Leaves-first
/// order so dependents stop before their dep.
#[tauri::command]
pub fn disable_plugin(state: State<'_, AppState>, name: String) -> Result<Vec<String>, AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.disable_plugin(&name)?)
}

/// Start a specific scheduler action for a plugin.
#[tauri::command]
pub fn start_plugin_scheduler(
    state: State<'_, AppState>,
    name: String,
    action: String,
) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.start_plugin_scheduler(&name, &action)?)
}

/// Stop a specific scheduler action for a plugin.
#[tauri::command]
pub fn stop_plugin_scheduler(
    state: State<'_, AppState>,
    name: String,
    action: String,
) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.stop_plugin_scheduler(&name, &action)?)
}

// ---------------------------------------------------------------------------
// App focus / active-tab state — called by the frontend on visibility changes
// ---------------------------------------------------------------------------

/// Notify the backend whether the app window currently has focus.
///
/// Snapshot of the boot loader's current state. The splash component polls
/// this on mount as a safety net for dev-mode HMR remounts where the listener
/// attaches after `arbor://boot-done` has already fired (the `frontend_ready`
/// handshake covers first-launch; this covers re-mount).
#[tauri::command]
pub fn get_boot_state(state: State<'_, AppState>) -> serde_json::Value {
    let done = state.boot_done.load(Ordering::Acquire);
    let progress = state.boot_progress.lock().ok().and_then(|g| g.clone());
    serde_json::json!({
        "done":     done,
        "progress": progress,
    })
}

/// Frontend handshake — `BootSplash.onMount` calls this once both the
/// `arbor://boot-progress` and `arbor://boot-done` listeners are registered.
/// The boot thread parks on `state.frontend_ready` until this flips, then
/// emits its events. Idempotent: subsequent calls are a no-op.
#[tauri::command]
pub fn frontend_ready(state: State<'_, AppState>) {
    let (lock, cvar) = &*state.frontend_ready;
    if let Ok(mut ready) = lock.lock() {
        if !*ready {
            *ready = true;
            cvar.notify_all();
        }
    }
}

/// Two things happen when the focus state changes:
///  1. `app_focused` is updated so focus-gated plugin schedulers can skip
///     firing while the window is in the background.
///  2. On Windows, EcoQoS / Efficiency Mode is toggled so Task Manager shows
///     the green leaf icon while Arbor is not in the foreground.
#[tauri::command]
pub fn set_app_focus(state: State<'_, AppState>, focused: bool) {
    // Update the focus flag so focus-gated plugin schedulers can skip firing
    // while the window is in the background.
    // Note: OS power-throttling (EcoQoS) is applied by the native
    // WindowEvent::Focused handler in lib.rs, which is more reliable than
    // going through the IPC round-trip.
    let t0 = std::time::Instant::now();
    let prev = state.app_focused.swap(focused, Ordering::Relaxed);
    tracing::info!(
        target: "arbor::focus",
        "set_app_focus(focused={focused}) prev={prev} took={}µs",
        t0.elapsed().as_micros()
    );
}

/// Inform the backend which tab is currently active in the frontend.
/// Used by `arbor.repo.fetch_active_tab()` to know which repo to operate on.
/// Also fires the `on_tab_switch` plugin hook when a real tab is activated.
#[tauri::command]
pub fn set_active_tab(state: State<'_, AppState>, tab_id: Option<String>) {
    if let Ok(mut id) = state.active_tab_id.lock() {
        *id = tab_id.clone();
    }
    if let Some(ref tid) = tab_id {
        // Look up the repo path so plugins can use arbor.settings.project correctly.
        // Lock repos, copy what we need, then drop before locking plugin_host.
        let repo_info: Option<(String, String)> = state.lock_repos().ok().and_then(|mut mgr| {
            mgr.get(tid).ok().map(|r| (r.path.clone(), r.name.clone()))
        });
        state.fire_hook("on_tab_switch", serde_json::json!({
            "tab_id": tid,
            "path":   repo_info.as_ref().map(|(p, _)| p.as_str()).unwrap_or(""),
            "name":   repo_info.as_ref().map(|(_, n)| n.as_str()).unwrap_or(""),
        }));
    }
}
