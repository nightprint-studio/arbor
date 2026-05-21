use std::sync::atomic::Ordering;

use tauri::{Emitter, State};
use serde::Serialize;

use crate::error::AppError;
use crate::plugin::runtime::PluginManifest;
use crate::plugin::toolchain::ToolchainEntry;
use crate::AppState;

// ---------------------------------------------------------------------------
// Plugin settings helpers — delegate to the shared settings_store module
// ---------------------------------------------------------------------------

// Settings now live in `global.json` (written through the
// `arbor.settings.global.set` Lua API). The legacy `settings.json` file
// owned by the old `[[setting]]` schema is gone — clearing a plugin's
// stored data therefore means clearing its `global.json`.
fn load_plugin_settings(plugin_name: &str) -> serde_json::Map<String, serde_json::Value> {
    let path = crate::plugin::settings_store::global_settings_path(plugin_name);
    crate::plugin::settings_store::load_settings_file(&path)
}

fn save_plugin_settings(plugin_name: &str, map: &serde_json::Map<String, serde_json::Value>) {
    let path = crate::plugin::settings_store::global_settings_path(plugin_name);
    crate::plugin::settings_store::save_settings_file(&path, map);
}

#[tauri::command]
pub fn list_plugins(_state: State<'_, AppState>) -> Result<Vec<PluginManifest>, AppError> {
    crate::plugin::runtime::discover_plugins()
}

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
pub fn get_plugins_enabled(state: State<'_, AppState>) -> Result<bool, AppError> {
    let cfg = state.lock_config()?;
    Ok(cfg.plugins_enabled)
}

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

/// Return the absolute path of the user's plugins directory so the UI can
/// reveal it in the OS file explorer. Path is NOT guaranteed to exist yet —
/// the frontend should create it before opening if that matters.
#[tauri::command]
pub fn get_plugin_directory() -> Result<String, AppError> {
    let dir = crate::plugin::runtime::plugin_dir();
    // Try to ensure the directory exists so opening it in the explorer
    // doesn't fail when the user has never installed a plugin. Errors are
    // non-fatal — if creation fails we still return the path and let the
    // caller decide how to handle "missing" state.
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir.to_string_lossy().to_string())
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
        if let Ok(host) = state.lock_plugin_host() {
            for (tab_id, path, name) in &opens {
                let ctx = serde_json::json!({
                    "tab_id": tab_id,
                    "path":   path,
                    "name":   name,
                });
                let _ = host.fire_hook("on_repo_open", &ctx.to_string());
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
                    let ctx = serde_json::json!({
                        "tab_id": tab_id,
                        "path":   path,
                        "name":   name,
                    });
                    let _ = host.fire_hook("on_tab_switch", &ctx.to_string());
                }
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
    let host = state.lock_plugin_host()?;
    host.fire_hook(&hook, &context_json)
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
    host.fire_hook_on(&plugin_name, &action, &context_json)
}

#[tauri::command]
pub fn enable_plugin(state: State<'_, AppState>, name: String) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    host.enable_plugin(&name)
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

#[tauri::command]
pub fn disable_plugin(state: State<'_, AppState>, name: String) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    host.disable_plugin(&name)
}

#[tauri::command]
pub fn list_plugin_info(state: State<'_, AppState>) -> Result<Vec<crate::plugin::runtime::PluginInfo>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.list_plugin_info())
}

/// A single node in the dependency graph returned to the frontend.
#[derive(Serialize)]
pub struct DepGraphNode {
    pub name:    String,
    pub version: String,
    pub enabled: bool,
    /// Plugins this one declared dependencies on (resolved to loaded plugins only).
    pub depends_on: Vec<DepGraphEdge>,
    /// Plugins that depend on this one.
    pub dependents: Vec<DepGraphEdge>,
    /// Dependency resolution error reported at load time, if any.
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct DepGraphEdge {
    pub name:     String,
    pub version:  String,
    pub optional: bool,
    /// true when the declared version requirement isn't satisfied by the loaded version.
    pub unmet:    bool,
}

/// Return the full plugin dependency graph, including unresolved edges.
#[tauri::command]
pub fn plugin_dep_graph(state: State<'_, AppState>) -> Result<Vec<DepGraphNode>, AppError> {
    use std::collections::HashMap;

    let host = state.lock_plugin_host()?;
    // Map name -> (version, enabled, declared deps)
    let mut entries: HashMap<String, (String, bool, Vec<crate::plugin::runtime::PluginDependency>, Option<String>)> = HashMap::new();
    for p in &host.plugins {
        entries.insert(
            p.manifest.name.clone(),
            (p.manifest.version.clone(), p.is_enabled(), p.manifest.dependencies.clone(), None),
        );
    }
    for d in &host.dormant {
        // Dormant plugins were skipped at startup but their dependency edges
        // still matter for the graph view: the user needs to see why nothing
        // depending on them resolved.
        entries.entry(d.manifest.name.clone()).or_insert((
            d.manifest.version.clone(),
            false,
            d.manifest.dependencies.clone(),
            None,
        ));
    }
    for f in &host.load_failures {
        // Load failures don't expose their declared deps (we only kept the
        // reason + identity). Surface them with empty deps + the error.
        entries.entry(f.name.clone()).or_insert((f.version.clone(), false, Vec::new(), Some(f.error.clone())));
    }

    // Pre-compute dependents.
    let mut dependents: HashMap<String, Vec<DepGraphEdge>> = HashMap::new();
    for (name, (_, _, deps, _)) in &entries {
        for d in deps {
            let unmet = entries.get(&d.name).map(|(v, _, _, _)| {
                if d.version.is_empty() { return false; }
                let ok = semver::VersionReq::parse(&d.version)
                    .ok()
                    .zip(semver::Version::parse(v).ok())
                    .map(|(req, vv)| req.matches(&vv))
                    .unwrap_or(true);
                !ok
            }).unwrap_or(!d.optional); // missing + not optional → unmet
            dependents.entry(d.name.clone()).or_default().push(DepGraphEdge {
                name:    name.clone(),
                version: entries.get(name).map(|(v, _, _, _)| v.clone()).unwrap_or_default(),
                optional: d.optional,
                unmet,
            });
        }
    }

    let mut out: Vec<DepGraphNode> = entries.iter().map(|(name, (version, enabled, deps, error))| {
        let depends_on: Vec<DepGraphEdge> = deps.iter().map(|d| {
            let unmet = match entries.get(&d.name) {
                None => !d.optional,
                Some((v, _, _, _)) => {
                    if d.version.is_empty() { false }
                    else {
                        let ok = semver::VersionReq::parse(&d.version)
                            .ok()
                            .zip(semver::Version::parse(v).ok())
                            .map(|(req, vv)| req.matches(&vv))
                            .unwrap_or(true);
                        !ok
                    }
                }
            };
            DepGraphEdge {
                name:    d.name.clone(),
                version: d.version.clone(),
                optional: d.optional,
                unmet,
            }
        }).collect();

        DepGraphNode {
            name:    name.clone(),
            version: version.clone(),
            enabled: *enabled,
            depends_on,
            dependents: dependents.get(name).cloned().unwrap_or_default(),
            error: error.clone(),
        }
    }).collect();

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Return the list of currently-enabled plugins that directly depend on `name`.
/// Used by the frontend to warn the user before disabling a plugin.
#[tauri::command]
pub fn plugin_dependents(state: State<'_, AppState>, name: String) -> Result<Vec<String>, AppError> {
    let host = state.lock_plugin_host()?;
    let mut out = Vec::new();
    for p in &host.plugins {
        if !p.is_enabled() { continue; }
        if p.manifest.name == name { continue; }
        if p.manifest.dependencies.iter().any(|d| d.name == name && !d.optional) {
            out.push(p.manifest.name.clone());
        }
    }
    out.sort();
    Ok(out)
}

/// Start a specific scheduler action for a plugin.
#[tauri::command]
pub fn start_plugin_scheduler(
    state: State<'_, AppState>,
    name: String,
    action: String,
) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    host.start_plugin_scheduler(&name, &action)
}

/// Stop a specific scheduler action for a plugin.
#[tauri::command]
pub fn stop_plugin_scheduler(
    state: State<'_, AppState>,
    name: String,
    action: String,
) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    host.stop_plugin_scheduler(&name, &action)
}

// ---------------------------------------------------------------------------
// Plugin settings — frontend read/write
// ---------------------------------------------------------------------------

/// Return all stored settings for a plugin as a JSON object.
#[tauri::command]
pub fn plugin_settings_get(
    _state: State<'_, AppState>,
    name: String,
) -> Result<serde_json::Map<String, serde_json::Value>, AppError> {
    Ok(load_plugin_settings(&name))
}

/// Overwrite all settings for a plugin with the provided JSON object.
#[tauri::command]
pub fn plugin_settings_set_all(
    _state: State<'_, AppState>,
    name: String,
    values: serde_json::Map<String, serde_json::Value>,
) -> Result<(), AppError> {
    save_plugin_settings(&name, &values);
    Ok(())
}

// ---------------------------------------------------------------------------
// App focus / active-tab state — called by the frontend on visibility changes
// ---------------------------------------------------------------------------

/// Notify the backend whether the app window currently has focus.
///
/// Snapshot of the boot loader's current state.  The splash component calls
/// this on mount to recover from the dev-mode race where the WebView is not
/// yet ready when `arbor://boot-done` is emitted — without it the splash
/// would sit through the 10s fallback timeout even though plugin load
/// completed in ~250ms.
#[tauri::command]
pub fn get_boot_state(state: State<'_, AppState>) -> serde_json::Value {
    let done = state.boot_done.load(Ordering::Acquire);
    let progress = state.boot_progress.lock().ok().and_then(|g| g.clone());
    serde_json::json!({
        "done":     done,
        "progress": progress,
    })
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
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({
                "tab_id": tid,
                "path":   repo_info.as_ref().map(|(p, _)| p.as_str()).unwrap_or(""),
                "name":   repo_info.as_ref().map(|(_, n)| n.as_str()).unwrap_or(""),
            });
            let _ = host.fire_hook("on_tab_switch", &ctx.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Toolchain commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_toolchains(
    state: State<'_, AppState>,
    kind: String,
) -> Result<Vec<ToolchainEntry>, AppError> {
    Ok(state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .list(&kind))
}

#[tauri::command]
pub fn add_toolchain(
    state: State<'_, AppState>,
    kind:  String,
    entry: ToolchainEntry,
) -> Result<(), AppError> {
    state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .add(&kind, entry);
    Ok(())
}

#[tauri::command]
pub fn remove_toolchain(
    state: State<'_, AppState>,
    kind: String,
    id:   String,
) -> Result<(), AppError> {
    state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .remove(&kind, &id);
    Ok(())
}

#[tauri::command]
pub fn set_active_toolchain(
    state: State<'_, AppState>,
    kind: String,
    id:   String,
) -> Result<(), AppError> {
    state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .set_active(&kind, &id);
    Ok(())
}

#[tauri::command]
pub fn detect_toolchains(
    state: State<'_, AppState>,
    kind: String,
) -> Result<Vec<ToolchainEntry>, AppError> {
    Ok(state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .detect(&kind))
}

// ---------------------------------------------------------------------------
// Contribution + tree + icon registry exposure
// ---------------------------------------------------------------------------

/// All contributions, optionally filtered by point name. The frontend uses
/// this to render plugin-driven UI slots (toolbar buttons, node actions,
/// decorators, …) consumed by built-in components like `PluginTreeSidebar`.
#[tauri::command]
pub fn list_plugin_contributions(
    state: State<'_, AppState>,
    point: Option<String>,
) -> Result<Vec<crate::plugin::contribution::PluginContribution>, AppError> {
    let host = state.lock_plugin_host()?;
    let items = match point {
        Some(p) => host.contributions.list_for_point(&p),
        None    => host.contributions.list_all(),
    };
    Ok(items)
}

/// Declared contribution points (informational). Useful for plugin authors to
/// inspect available extension slots from the docs panel.
#[tauri::command]
pub fn list_contribution_points(
    state: State<'_, AppState>,
) -> Result<Vec<crate::plugin::contribution::ContributionPoint>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.contributions.list_points())
}

// Tree snapshots and custom icons used to be exposed via dedicated IPC
// commands (`get_plugin_tree_state`, `get_plugin_icons`). Both are now
// retrieved by the frontend through the unified contribution registry —
// `list_plugin_contributions("arbor:tree-state")` /
// `list_plugin_contributions("arbor:icon")` — so no parallel cache exists.

// ---------------------------------------------------------------------------
// Containers (Phase 2 — ContributableModal)
// ---------------------------------------------------------------------------

/// All containers registered via `arbor.ui.container.register`. The frontend
/// uses this to look up `title`, `layout`, `width`, etc. when an
/// `arbor://container-open` event fires.
#[tauri::command]
pub fn list_containers(
    state: State<'_, AppState>,
) -> Result<Vec<crate::plugin::contribution::ContainerDef>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.contributions.list_containers())
}

/// Single container by `<plugin>::<id>` key. Returns `None` if no plugin
/// has registered that key (e.g. plugin disabled / reloaded).
#[tauri::command]
pub fn get_container(
    state: State<'_, AppState>,
    key:   String,
) -> Result<Option<crate::plugin::contribution::ContainerDef>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.contributions.get_container(&key))
}
