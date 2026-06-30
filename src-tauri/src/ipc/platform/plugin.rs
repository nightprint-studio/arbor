//! `plugin` domain — the **leaf-clean** subset of the plugin command surface,
//! routed through the platform backend.
//!
//! Only config-backed reads/writes and pure reflection/metadata getters live
//! here. Each handler is the body its `#[tauri::command]` ran inline, now
//! self-registered under `program = "platform"`; behaviour (locks held, files
//! read/written, errors) is byte-identical. Commands that took no `AppState`
//! use `_state: &AppState` to satisfy the handler macro's context arg.
//!
//! Everything that mutates the plugin runtime (install/load/unload/reload/
//! enable/disable/uninstall), executes Lua, fires hooks, emits `arbor://*`
//! events, or takes an `AppHandle`/`Window` stays inline in
//! `commands::plugin_commands` as a keep-shell Tauri command. The boot-handshake
//! and focus/active-tab commands (`get_boot_state`, `frontend_ready`,
//! `set_app_focus`, `set_active_tab`) also stay inline: they aren't
//! config/reflection, several fire hooks, and they don't return `Result`.

use serde::Serialize;

use arbor_plugin_types::prelude::Manifest;

use arbor_plugin_core::prelude::ToolchainEntry;
use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

// ---------------------------------------------------------------------------
// Plugin settings helpers — delegate to the shared settings_store module
// ---------------------------------------------------------------------------

// Settings now live in `global.json` (written through the
// `arbor.settings.global.set` Lua API). The legacy `settings.json` file
// owned by the old `[[setting]]` schema is gone — clearing a plugin's
// stored data therefore means clearing its `global.json`.
fn load_plugin_settings(plugin_name: &str) -> serde_json::Map<String, serde_json::Value> {
    let path = arbor_plugin_core::prelude::global_settings_path(plugin_name);
    arbor_plugin_core::prelude::load_settings_file(&path)
}

fn save_plugin_settings(plugin_name: &str, map: &serde_json::Map<String, serde_json::Value>) {
    let path = arbor_plugin_core::prelude::global_settings_path(plugin_name);
    arbor_plugin_core::prelude::save_settings_file(&path, map);
}

#[platform::handler(program = "platform")]
fn list_plugins(_state: &AppState) -> Result<Vec<Manifest>, AppError> {
    Ok(arbor_plugin_core::prelude::discover_plugins()?)
}

/// Read the master plugin-system kill-switch (Plugin Manager toggle).
#[platform::handler(program = "platform")]
fn get_plugins_enabled(state: &AppState) -> Result<bool, AppError> {
    let cfg = state.lock_config()?;
    Ok(cfg.plugins_enabled)
}

/// Return the absolute path of the user's plugins directory so the UI can
/// reveal it in the OS file explorer. Path is NOT guaranteed to exist yet —
/// the frontend should create it before opening if that matters.
#[platform::handler(program = "platform")]
fn get_plugin_directory(_state: &AppState) -> Result<String, AppError> {
    let dir = arbor_plugin_core::prelude::plugin_dir();
    // Try to ensure the directory exists so opening it in the explorer
    // doesn't fail when the user has never installed a plugin. Errors are
    // non-fatal — if creation fails we still return the path and let the
    // caller decide how to handle "missing" state.
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir.to_string_lossy().to_string())
}

/// Resolve the on-disk folder of a discovered plugin by name. Walks the same
/// discovery roots as the host (`plugin_dir()` first, then the marketplace
/// install dir) and returns the directory whose manifest claims `name`. The
/// folder name on disk can differ from the manifest's `name` (e.g. zip imports
/// preserve the archive root), so the FE can't construct the path itself.
#[platform::handler(program = "platform")]
fn get_installed_plugin_path(_state: &AppState, name: String) -> Result<String, AppError> {
    let manifests = arbor_plugin_core::prelude::discover_plugins()?;
    let m = manifests.into_iter()
        .find(|m| m.name == name)
        .ok_or_else(|| AppError::Other(format!("plugin '{name}' is not installed")))?;
    Ok(m.dir.to_string_lossy().to_string())
}

/// Preview the enable cascade for `name`. `plan` is the ordered list of
/// plugins that would be enabled (deps first, target last); `blockers`
/// lists required deps that are missing, unloadable, or version-incompatible.
/// When `blockers` is non-empty, `enable_plugin` will refuse to run.
#[platform::handler(program = "platform")]
fn plugin_enable_preview(
    state: &AppState,
    name:  String,
) -> Result<arbor_plugin_core::prelude::EnablePreview, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(arbor_plugin_core::prelude::EnablePreview {
        plan:     host.compute_enable_cascade(&name),
        blockers: host.compute_enable_blockers(&name),
    })
}

/// Preview the disable cascade for `name`: every currently-enabled plugin
/// that (transitively) requires it, leaves-first, with `name` last.
/// Returns an empty list when `name` isn't currently enabled.
#[platform::handler(program = "platform")]
fn plugin_disable_preview(
    state: &AppState,
    name:  String,
) -> Result<Vec<String>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.compute_disable_cascade(&name))
}

#[platform::handler(program = "platform")]
fn list_plugin_info(state: &AppState) -> Result<Vec<arbor_plugin_core::prelude::PluginInfo>, AppError> {
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
#[platform::handler(program = "platform")]
fn plugin_dep_graph(state: &AppState) -> Result<Vec<DepGraphNode>, AppError> {
    use std::collections::HashMap;

    let host = state.lock_plugin_host()?;
    // Map name -> (version, enabled, declared deps)
    let mut entries: HashMap<String, (String, bool, Vec<arbor_plugin_types::prelude::Dependency>, Option<String>)> = HashMap::new();
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
#[platform::handler(program = "platform")]
fn plugin_dependents(state: &AppState, name: String) -> Result<Vec<String>, AppError> {
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

// ---------------------------------------------------------------------------
// Plugin settings — frontend read/write (plain file-backed, no host)
// ---------------------------------------------------------------------------

/// Return all stored settings for a plugin as a JSON object.
#[platform::handler(program = "platform")]
fn plugin_settings_get(
    _state: &AppState,
    name: String,
) -> Result<serde_json::Map<String, serde_json::Value>, AppError> {
    Ok(load_plugin_settings(&name))
}

/// Overwrite all settings for a plugin with the provided JSON object.
#[platform::handler(program = "platform")]
fn plugin_settings_set_all(
    _state: &AppState,
    name: String,
    values: serde_json::Map<String, serde_json::Value>,
) -> Result<(), AppError> {
    save_plugin_settings(&name, &values);
    Ok(())
}

// ---------------------------------------------------------------------------
// Toolchain registry — config-backed registry reads/writes
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn list_toolchains(
    state: &AppState,
    kind: String,
) -> Result<Vec<ToolchainEntry>, AppError> {
    Ok(state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .list(&kind))
}

#[platform::handler(program = "platform")]
fn add_toolchain(
    state: &AppState,
    kind:  String,
    entry: ToolchainEntry,
) -> Result<(), AppError> {
    state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .add(&kind, entry);
    Ok(())
}

#[platform::handler(program = "platform")]
fn remove_toolchain(
    state: &AppState,
    kind: String,
    id:   String,
) -> Result<(), AppError> {
    state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .remove(&kind, &id);
    Ok(())
}

#[platform::handler(program = "platform")]
fn set_active_toolchain(
    state: &AppState,
    kind: String,
    id:   String,
) -> Result<(), AppError> {
    state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .set_active(&kind, &id);
    Ok(())
}

#[platform::handler(program = "platform")]
fn detect_toolchains(
    state: &AppState,
    kind: String,
) -> Result<Vec<ToolchainEntry>, AppError> {
    Ok(state.toolchain_registry
        .lock().map_err(|_| AppError::Other("toolchain mutex poisoned".into()))?
        .detect(&kind))
}

// ---------------------------------------------------------------------------
// Contribution + container registry exposure — pure reflection
// ---------------------------------------------------------------------------

/// All contributions, optionally filtered by point name. The frontend uses
/// this to render plugin-driven UI slots (toolbar buttons, node actions,
/// decorators, …) consumed by built-in components like `PluginTreeSidebar`.
#[platform::handler(program = "platform")]
fn list_plugin_contributions(
    state: &AppState,
    point: Option<String>,
) -> Result<Vec<arbor_plugin_core::prelude::PluginContribution>, AppError> {
    let host = state.lock_plugin_host()?;
    let items = match point {
        Some(p) => host.contributions.list_for_point(&p),
        None    => host.contributions.list_all(),
    };
    Ok(items)
}

/// Declared contribution points (informational). Useful for plugin authors to
/// inspect available extension slots from the docs panel.
#[platform::handler(program = "platform")]
fn list_contribution_points(
    state: &AppState,
) -> Result<Vec<arbor_plugin_core::prelude::ContributionPoint>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.contributions.list_points())
}

/// All containers registered via `arbor.ui.container.register`. The frontend
/// uses this to look up `title`, `layout`, `width`, etc. when an
/// `arbor://container-open` event fires.
#[platform::handler(program = "platform")]
fn list_containers(
    state: &AppState,
) -> Result<Vec<arbor_plugin_core::prelude::ContainerDef>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.contributions.list_containers())
}

/// Single container by `<plugin>::<id>` key. Returns `None` if no plugin
/// has registered that key (e.g. plugin disabled / reloaded).
#[platform::handler(program = "platform")]
fn get_container(
    state: &AppState,
    key:   String,
) -> Result<Option<arbor_plugin_core::prelude::ContainerDef>, AppError> {
    let host = state.lock_plugin_host()?;
    Ok(host.contributions.get_container(&key))
}

// ===========================================================================
// emit/seam pass — plugin-runtime mutations moved off `AppHandle`.
//
// These mutate the plugin runtime / fire hooks and used to take an `AppHandle`
// solely to emit `arbor://plugins-reloaded`; that emit now goes through the
// backend event sink (`state.emit`). They return `Result`, so the handler macro
// accepts them — unlike the boot/focus handshake commands (`get_boot_state`,
// `frontend_ready`, `set_app_focus`, `set_active_tab`), which return no `Result`
// and stay inline in `commands::plugin_commands`.
// ===========================================================================

/// Shared reload: cancel plugin jobs, reload the host + restart its schedulers,
/// re-fire `on_repo_open` for every open tab (plus `on_tab_switch` for the
/// active one so plugins that derive `current_repo` from the last lifecycle
/// event land on the right tab), then broadcast `arbor://plugins-reloaded`.
/// Used by `reload_plugins` and the "enable" branch of `set_plugins_enabled`,
/// and by the live profile switch (`commands::profile_commands::switch_profile`)
/// to pick up the new profile's plugin set.
pub(crate) fn reload_runtime(state: &AppState) -> Result<(), AppError> {
    // Cancel all running plugin jobs before reloading so stale processes don't linger.
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.cancel_by_plugin(None);
    }
    {
        let mut host = state.lock_plugin_host()?;
        host.reload()?;
        host.start_all_schedulers();
    } // release lock before firing hooks / emitting

    // corvus-be owns the open-tab registry; ask it for `(tab_id, path, name)`.
    let opens: Vec<(String, String, String)> = crate::ipc::open_repo_tabs(state);
    if !opens.is_empty() {
        for (tab_id, path, name) in &opens {
            state.fire_hook("on_repo_open", serde_json::json!({
                "tab_id": tab_id, "path": path, "name": name,
            }));
        }
        // `list_open()` order is non-deterministic; fire one final `on_tab_switch`
        // for the active tab so plugins keyed on the last event land correctly.
        let active_tab = state.active_tab_id.lock().ok().and_then(|g| g.clone());
        if let Some(tid) = active_tab {
            if let Some((tab_id, path, name)) = opens.iter().find(|(t, _, _)| t == &tid) {
                state.fire_hook("on_tab_switch", serde_json::json!({
                    "tab_id": tab_id, "path": path, "name": name,
                }));
            }
        }
    }

    state.emit("arbor://plugins-reloaded", ());
    Ok(())
}

/// Master plugin-system kill-switch. Persists the choice, then either reloads
/// the runtime from disk (`enabled`) or tears it down (`!enabled`).
#[platform::handler(program = "platform")]
fn set_plugins_enabled(state: &AppState, enabled: bool) -> Result<(), AppError> {
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
        reload_runtime(state)?;
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
        state.emit("arbor://plugins-reloaded", ());
    }
    Ok(())
}

#[platform::handler(program = "platform")]
fn reload_plugins(state: &AppState) -> Result<(), AppError> {
    reload_runtime(state)
}

#[platform::handler(program = "platform")]
fn exec_hook(state: &AppState, hook: String, context_json: String) -> Result<(), AppError> {
    let ctx: serde_json::Value =
        serde_json::from_str(&context_json).unwrap_or_else(|_| serde_json::json!({}));
    state.fire_hook(&hook, ctx);
    Ok(())
}

/// Fire a specific action on a specific plugin (declarative UI element click).
#[platform::handler(program = "platform")]
fn fire_plugin_action(
    state: &AppState,
    plugin_name: String,
    action: String,
    context_json: String,
) -> Result<(), AppError> {
    let host = state.lock_plugin_host()?;
    arbor_plugin_core::prelude::fire_on(&host, &plugin_name, &action, &context_json);
    Ok(())
}

/// Invoke a registered command on behalf of `caller_plugin` (the declarative
/// `kind = "command"` dispatch path; capability gates live in the host).
#[platform::handler(program = "platform")]
fn fire_command(
    state: &AppState,
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

/// Enable a plugin (transitive required deps + target, deps-first). Errors when
/// a required dep is missing/unloadable — call `plugin_enable_preview` first.
#[platform::handler(program = "platform")]
fn enable_plugin(state: &AppState, name: String) -> Result<Vec<String>, AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.enable_plugin(&name)?)
}

/// Disable a plugin + every transitively-required dependent (leaves-first).
#[platform::handler(program = "platform")]
fn disable_plugin(state: &AppState, name: String) -> Result<Vec<String>, AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.disable_plugin(&name)?)
}

/// Uninstall a plugin: remove its folder, global data, persisted state, and
/// per-repo `.arbor/plugins/<name>/` across open tabs + the registry. Returns
/// non-fatal warnings (paths that couldn't be removed); in-memory state is
/// always cleared.
#[platform::handler(program = "platform")]
fn delete_plugin(state: &AppState, name: String) -> Result<Vec<String>, AppError> {
    // Cancel running jobs from this plugin before tearing it down.
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.cancel_by_plugin(Some(&name));
    }

    // Collect every repo path we should clean — open tabs + every registered repo
    // (both resolved from corvus-be, the registry owner) — before locking the
    // plugin host (avoid holding two mutexes).
    let mut repo_paths: Vec<String> = crate::ipc::open_repo_paths(state);
    repo_paths.extend(crate::ipc::registry_repo_paths(state));
    repo_paths.sort();
    repo_paths.dedup();

    let warnings = {
        let mut host = state.lock_plugin_host()?;
        host.delete_plugin(&name, &repo_paths)?
    };

    state.emit("arbor://plugins-reloaded", ());
    Ok(warnings)
}

/// Start a specific scheduler action for a plugin.
#[platform::handler(program = "platform")]
fn start_plugin_scheduler(state: &AppState, name: String, action: String) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.start_plugin_scheduler(&name, &action)?)
}

/// Stop a specific scheduler action for a plugin.
#[platform::handler(program = "platform")]
fn stop_plugin_scheduler(state: &AppState, name: String, action: String) -> Result<(), AppError> {
    let mut host = state.lock_plugin_host()?;
    Ok(host.stop_plugin_scheduler(&name, &action)?)
}
