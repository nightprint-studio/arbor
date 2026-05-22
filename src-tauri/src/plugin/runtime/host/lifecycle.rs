//! Reload, enable, disable, delete — the full lifecycle of a plugin entry
//! plus the `load_plugin()` helper that builds a fresh Lua VM.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::error::{AppError, Result};

use super::PluginHost;
use crate::plugin::runtime::consts::{ARBOR_API_VERSION, ARBOR_APP_VERSION};
use crate::plugin::runtime::loaded::{DormantPlugin, LoadedPlugin, TimerCancels, TimerCounter};
use crate::plugin::runtime::manifest::{
    PluginManifest, discover_plugins_detailed, load_plugin_states, plugin_dir,
    save_plugin_states, topo_sort_manifests,
};
use crate::plugin::runtime::manifest::deps::PluginLoadFailure;
use crate::plugin::runtime::manifest::schedule::ScheduleRegistry;

impl PluginHost {
    /// Record a plugin-load failure in every surface the user can see:
    ///   1. terminal log (tracing::warn)
    ///   2. `self.load_failures` so `list_plugins` / dependency-graph IPCs
    ///      include it with `dep_error` populated → Plugin Manager renders
    ///      a red "Failed to load" entry
    ///   3. the Lua log panel (`plugin_logs::record`) so users who already
    ///      have that panel open see the error inline
    ///   4. a transient error notification so users not staring at the
    ///      Plugin Manager still know something went wrong
    ///
    /// Used by both startup `reload()` and on-demand `enable_plugin()` —
    /// previously both silently dropped the error to the terminal log,
    /// making activation-failures invisible from inside Arbor.
    fn record_load_failure(&mut self, manifest: &PluginManifest, error: &str) {
        let name = &manifest.name;
        tracing::warn!("failed to load plugin '{name}': {error}");

        self.load_failures.retain(|f| f.name != *name);
        self.load_failures.push(PluginLoadFailure {
            name:        name.clone(),
            version:     manifest.version.clone(),
            description: manifest.description.clone(),
            author:      manifest.author.clone(),
            error:       error.to_string(),
        });

        if let Some(ref h) = self.app_handle {
            crate::plugin_logs::record(
                h, "error", name,
                format!("Plugin failed to load: {error}"),
            );
            // Best-effort UI notification. Same channel as `arbor.notify`,
            // so it appears in the StatusBar bell + dismissable list.
            use tauri::Emitter;
            let _ = h.emit("plugin:notification", serde_json::json!({
                "plugin_name": name,
                "title":       format!("Plugin '{name}' failed to load"),
                "message":     error,
                "level":       "error",
            }));
        }
    }

    // -----------------------------------------------------------------------
    // Reload
    // -----------------------------------------------------------------------

    pub fn reload(&mut self) -> Result<()> {
        self.unload_all();

        let states = load_plugin_states();
        let (all_manifests, bad_manifests) = discover_plugins_detailed()?;

        // Surface manifest parse failures the same way we surface load
        // failures: list them in the Plugin Manager AND drop a line in the
        // Plugin Logs panel so the author isn't left guessing why their
        // plugin folder is being ignored.
        for bad in bad_manifests {
            tracing::warn!(
                "plugin folder '{}' skipped: manifest parse error — {}",
                bad.folder_name, bad.error
            );
            self.load_failures.retain(|f| f.name != bad.folder_name);
            self.load_failures.push(PluginLoadFailure {
                name:        bad.folder_name.clone(),
                version:     String::new(),
                description: String::new(),
                author:      String::new(),
                error:       format!("plugin.toml parse error: {}", bad.error),
            });
            if let Some(ref h) = self.app_handle {
                crate::plugin_logs::record(
                    h, "error", &bad.folder_name,
                    format!("plugin.toml parse error: {}", bad.error),
                );
            }
        }

        // Sort topologically so dependencies are loaded before dependents.
        let (sorted, cycle_names) = topo_sort_manifests(all_manifests);
        for name in cycle_names {
            tracing::error!("plugin '{name}' is in a dependency cycle — skipping");
            self.load_failures.push(PluginLoadFailure {
                name:        name.clone(),
                version:     String::new(),
                description: String::new(),
                author:      String::new(),
                error:       format!("Cyclic dependency detected involving '{name}'"),
            });
        }

        // Boot-progress wiring. The frontend listens for `arbor://boot-progress`
        // events and renders a splash overlay until the matching
        // `arbor://boot-done` arrives. We emit one event per manifest right
        // before the (potentially slow) `load_plugin` call so the UI shows
        // the name of whatever is currently being parsed/initialised.
        let total = sorted.len();
        if let Some(ref h) = self.app_handle {
            use tauri::Emitter;
            let _ = h.emit("arbor://boot-progress", serde_json::json!({
                "phase":   "starting",
                "name":    "",
                "current": 0,
                "total":   total,
                "message": format!("Loading {} plugins…", total),
            }));
        }

        // Track name → version for plugins that successfully loaded.
        let mut loaded_versions: HashMap<String, String> = HashMap::new();

        // Per-plugin load-time accounting so the worst offenders are easy to
        // spot in the terminal. The splash also surfaces the previous
        // plugin's duration in its next `arbor://boot-progress` event so the
        // user doesn't need a terminal to see the slow one.
        let reload_started = std::time::Instant::now();
        let mut timings: Vec<(String, u128)> = Vec::new();
        let mut last_timing: Option<(String, u128)> = None;

        for (idx, manifest) in sorted.into_iter().enumerate() {
            let name = manifest.name.clone();

            if let Some(ref h) = self.app_handle {
                use tauri::Emitter;
                let prev_note = match &last_timing {
                    Some((prev_name, ms)) => {
                        format!(" (prev: '{}' {}ms)", prev_name, ms)
                    }
                    None => String::new(),
                };
                let _ = h.emit("arbor://boot-progress", serde_json::json!({
                    "phase":   "loading-plugin",
                    "name":    name,
                    "current": idx + 1,
                    "total":   total,
                    "message": format!("Loading plugin '{}'…{}", name, prev_note),
                }));
            }

            // Check each declared dependency.
            let mut dep_error: Option<String> = None;
            'dep_check: for dep in &manifest.dependencies {
                match loaded_versions.get(&dep.name) {
                    None => {
                        if !dep.optional {
                            dep_error = Some(format!(
                                "required dependency '{}' is not loaded", dep.name
                            ));
                            break 'dep_check;
                        }
                        tracing::warn!(
                            "plugin '{name}': optional dependency '{}' not found — continuing",
                            dep.name
                        );
                    }
                    Some(loaded_ver) if !dep.version.is_empty() => {
                        let ok = semver::VersionReq::parse(&dep.version)
                            .ok()
                            .zip(semver::Version::parse(loaded_ver).ok())
                            .map(|(req, v)| req.matches(&v))
                            .unwrap_or(true); // be permissive when semver strings malformed
                        if !ok {
                            let msg = format!(
                                "dependency '{}' v{} does not satisfy requirement '{}'",
                                dep.name, loaded_ver, dep.version
                            );
                            if dep.optional {
                                tracing::warn!("plugin '{name}': {msg} (optional — continuing)");
                            } else {
                                dep_error = Some(msg);
                                break 'dep_check;
                            }
                        }
                    }
                    _ => {} // loaded, no version constraint or version satisfied
                }
            }

            if let Some(err) = dep_error {
                tracing::warn!("plugin '{name}' skipped: {err}");
                self.load_failures.push(PluginLoadFailure {
                    name:        name.clone(),
                    version:     manifest.version.clone(),
                    description: manifest.description.clone(),
                    author:      manifest.author.clone(),
                    error:       err,
                });
                continue;
            }

            let version = manifest.version.clone();
            let want_enabled = *states.get(&name).unwrap_or(&false);

            // (B) Skip-load: plugins disabled in plugin_states.json never get
            // a Lua VM. They're parked in `self.dormant` so the Plugin Manager
            // still lists them, and `enable_plugin()` knows how to promote
            // them by calling `load_plugin()` lazily.
            if !want_enabled {
                loaded_versions.insert(name.clone(), version);
                self.dormant.push(DormantPlugin { manifest });
                continue;
            }

            // Snapshot the manifest BEFORE moving it into `load_plugin` so
            // `record_load_failure` can attach the version/description/author
            // metadata to the failure entry on error.
            let manifest_snapshot = manifest.clone();
            let plugin_started = std::time::Instant::now();
            let result = load_plugin(
                manifest,
                self.app_handle.clone(),
                self.contributions.clone(),
                self.tree_store.clone(),
                self.icon_registry.clone(),
            );
            let plugin_ms = plugin_started.elapsed().as_millis();
            timings.push((name.clone(), plugin_ms));
            last_timing = Some((name.clone(), plugin_ms));
            tracing::info!("plugin '{name}' loaded in {plugin_ms}ms");
            match result {
                Ok(p) => {
                    loaded_versions.insert(name.clone(), version);
                    self.plugins.push(p);
                }
                Err(e) => {
                    self.record_load_failure(&manifest_snapshot, &e.to_string());
                }
            }
        }

        // Summary: total wall time + top-5 slowest plugins, so the user can
        // see at a glance whether the splash fallback (10s) was triggered by
        // one fat plugin or by death-by-a-thousand-paper-cuts.
        let total_ms = reload_started.elapsed().as_millis();
        let mut sorted_timings = timings.clone();
        sorted_timings.sort_by(|a, b| b.1.cmp(&a.1));
        let top: Vec<String> = sorted_timings.iter().take(5)
            .map(|(n, ms)| format!("{n}={ms}ms"))
            .collect();
        if total_ms > 3_000 {
            tracing::warn!(
                "plugin reload took {total_ms}ms for {} plugins — slowest: {}",
                timings.len(), top.join(", ")
            );
        } else {
            tracing::info!(
                "plugin reload took {total_ms}ms for {} plugins — slowest: {}",
                timings.len(), top.join(", ")
            );
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Enable / Disable
    // -----------------------------------------------------------------------

    pub fn enable_plugin(&mut self, name: &str) -> Result<()> {
        // Branch 1 — plugin already in `self.plugins` (was disabled mid-session
        // and the Lua VM is still alive). Flip the flag, re-arm schedulers,
        // and re-fire `on_plugin_load` so any side-effecting setup the plugin
        // does there (e.g. `arbor.ui.set_branding`, theme overlays, sidebar
        // content) is reapplied — disable already fired `on_plugin_unload`,
        // so the pair has to balance for stateful plugins to come back clean.
        if let Some(plugin) = self.plugins.iter_mut().find(|p| p.manifest.name == name) {
            plugin.enabled.store(true, Ordering::Relaxed);
            let scheduler_on = plugin.manifest.scheduler.enabled;
            let schedules = if scheduler_on {
                plugin.schedules.lock().map(|g| g.clone()).unwrap_or_default()
            } else {
                Vec::new()
            };

            let mut states = load_plugin_states();
            states.insert(name.to_string(), true);
            save_plugin_states(&states);

            // Symmetric counterpart to `disable_plugin`'s on_plugin_unload.
            let ctx = serde_json::json!({
                "plugin_name": name,
                "dir":         plugin.manifest.dir.to_string_lossy(),
                "api_version": plugin.manifest.arbor_api,
            });
            let _ = crate::plugin::hook_registry::fire(
                &plugin.lua, "on_plugin_load", &ctx.to_string(),
            );

            for sched in schedules {
                self.spawn_scheduler(name, &sched);
            }
            return Ok(());
        }

        // Branch 2 — plugin is dormant (skipped at startup because it was
        // disabled). Promote it: build the Lua VM now via `load_plugin`, run
        // main.lua, fire on_plugin_load, then arm schedulers.
        let dormant_idx = self.dormant.iter().position(|d| d.manifest.name == name)
            .ok_or_else(|| AppError::Other(format!("plugin '{name}' not found")))?;
        let manifest = self.dormant.remove(dormant_idx).manifest;
        // Keep a snapshot for failure surfacing — `load_plugin` consumes
        // the original. Also lets us flip plugin_states back to false so
        // the next refresh doesn't pointlessly retry a known-broken load.
        let manifest_snapshot = manifest.clone();

        let loaded = match load_plugin(
            manifest,
            self.app_handle.clone(),
            self.contributions.clone(),
            self.tree_store.clone(),
            self.icon_registry.clone(),
        ) {
            Ok(p)  => p,
            Err(e) => {
                let err_msg = e.to_string();
                self.record_load_failure(&manifest_snapshot, &err_msg);
                // Persist the broken plugin as disabled. Without this the
                // next `reload()` tries to load it again silently because
                // `enable_plugin`'s `states.insert(true)` ran on the
                // previous activation and is still on disk.
                let mut states = load_plugin_states();
                states.insert(name.to_string(), false);
                save_plugin_states(&states);
                return Err(AppError::Other(format!("Plugin '{name}' failed to load: {err_msg}")));
            }
        };

        let scheduler_on = loaded.manifest.scheduler.enabled;
        let schedules = if scheduler_on {
            loaded.schedules.lock().map(|g| g.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        self.plugins.push(loaded);

        let mut states = load_plugin_states();
        states.insert(name.to_string(), true);
        save_plugin_states(&states);

        for sched in schedules {
            // `on_load` actions on schedules registered during main.lua are
            // a fresh-VM concept — fire them here so the first activation
            // matches the startup-load path.
            if sched.on_load {
                let _ = self.fire_hook_on(name, &sched.action, "{}");
            }
            self.spawn_scheduler(name, &sched);
        }
        Ok(())
    }

    pub fn disable_plugin(&mut self, name: &str) -> Result<()> {
        let plugin = self.plugins.iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| AppError::Other(format!("plugin '{name}' not found")))?;

        // Fire unload hook before disabling.
        let _ = crate::plugin::hook_registry::fire(&plugin.lua, "on_plugin_unload", "{}");

        plugin.enabled.store(false, Ordering::Relaxed);

        // Drop the plugin's contributions, tree snapshots, and custom icons so
        // disabled plugins immediately disappear from the UI.
        self.contributions.remove_plugin(name);
        self.tree_store.remove_plugin(name);
        self.icon_registry.remove_plugin(name);

        // Cancel all Lua timers for this plugin.
        if let Ok(tc) = plugin.timer_cancels.lock() {
            for cancel in tc.values() {
                cancel.store(true, Ordering::Relaxed);
            }
        }

        let mut states = load_plugin_states();
        states.insert(name.to_string(), false);
        save_plugin_states(&states);

        // Cancel all schedulers for this plugin.
        let keys: Vec<String> = self.scheduler_cancels.keys()
            .filter(|k| k.starts_with(&format!("{name}:")))
            .cloned()
            .collect();
        for k in keys {
            if let Some(cancel) = self.scheduler_cancels.remove(&k) {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Uninstall
    // -----------------------------------------------------------------------

    /// Permanently remove a plugin: drop it from the live host, delete its
    /// folder under `plugins/`, wipe `plugin_data[-dev]/<name>/`, drop its
    /// entry from `plugin_states[-dev].json`, and remove
    /// `<repo>/.arbor/plugins/<name>/` from every repo path passed in.
    ///
    /// `repo_paths` is supplied by the caller so we don't take a second mutex
    /// (RepoManager + RepoRegistry) while holding the plugin-host lock.
    /// Filesystem failures are collected and returned as warnings — the
    /// uninstall is best-effort once the in-memory state has been cleared.
    pub fn delete_plugin(&mut self, name: &str, repo_paths: &[String]) -> Result<Vec<String>> {
        let mut warnings: Vec<String> = Vec::new();

        // Step 1: detach the live plugin (if any) so its hooks/timers/
        // schedulers stop firing before we touch disk.
        if let Some(idx) = self.plugins.iter().position(|p| p.manifest.name == name) {
            let plugin = &self.plugins[idx];
            // Best-effort unload hook.
            let _ = crate::plugin::hook_registry::fire(&plugin.lua, "on_plugin_unload", "{}");
            // Cancel Lua timers.
            if let Ok(tc) = plugin.timer_cancels.lock() {
                for cancel in tc.values() { cancel.store(true, Ordering::Relaxed); }
            }
            // Cancel schedulers belonging to this plugin.
            let keys: Vec<String> = self.scheduler_cancels.keys()
                .filter(|k| k.starts_with(&format!("{name}:")))
                .cloned()
                .collect();
            for k in keys {
                if let Some(c) = self.scheduler_cancels.remove(&k) {
                    c.store(true, Ordering::Relaxed);
                }
            }
            // Drop contributed UI state.
            self.contributions.remove_plugin(name);
            self.tree_store.remove_plugin(name);
            self.icon_registry.remove_plugin(name);
            // Drop the LoadedPlugin (Lua VM gets dropped here).
            self.plugins.remove(idx);
        }
        // Plugins that failed to load also count as installed on disk.
        self.load_failures.retain(|f| f.name != name);
        // Dormant entries (disabled-at-startup, no live VM) — drop the manifest.
        self.dormant.retain(|d| d.manifest.name != name);

        // Step 2: remove the on-disk plugin folder.
        // We resolve via the manifest dir first when the plugin was loaded (so
        // dev plugins shipped from the workspace `plugins/` are honoured), and
        // fall back to `plugin_dir()/<name>` when we only have the name.
        let plugins_root = plugin_dir();
        let folder = plugins_root.join(name);
        if folder.exists() {
            if let Err(e) = std::fs::remove_dir_all(&folder) {
                warnings.push(format!("failed to remove {}: {e}", folder.display()));
            }
        }

        // Step 3: remove the global plugin_data folder for this plugin.
        let data_subdir = if cfg!(debug_assertions) { "plugin_data-dev" } else { "plugin_data" };
        let data_root = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("arbor")
            .join(data_subdir)
            .join(name);
        if data_root.exists() {
            if let Err(e) = std::fs::remove_dir_all(&data_root) {
                warnings.push(format!("failed to remove {}: {e}", data_root.display()));
            }
        }

        // Step 4: drop the entry from plugin_states[-dev].json.
        let mut states = load_plugin_states();
        if states.remove(name).is_some() {
            save_plugin_states(&states);
        }

        // Step 5: per-repo project settings cleanup.
        for repo in repo_paths {
            let dir = PathBuf::from(repo).join(".arbor").join("plugins").join(name);
            if dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    warnings.push(format!("failed to remove {}: {e}", dir.display()));
                }
            }
        }

        Ok(warnings)
    }
}

// ---------------------------------------------------------------------------
// Standalone loader — builds a fresh Lua VM, runs the entry script, fires
// `on_plugin_load`, and returns a `LoadedPlugin` ready to be pushed into
// `PluginHost::plugins`.
// ---------------------------------------------------------------------------

pub fn load_plugin(
    manifest: PluginManifest,
    app_handle: Option<tauri::AppHandle>,
    contributions: crate::plugin::contribution::ContributionRegistry,
    tree_store:    crate::plugin::tree::TreeStore,
    icon_registry: crate::plugin::tree::IconRegistry,
) -> Result<LoadedPlugin> {
    // API version compatibility check (integer contract — bumped on breaking changes).
    if manifest.arbor_api > ARBOR_API_VERSION {
        return Err(AppError::Plugin(format!(
            "plugin '{}' requires Arbor API v{} but this build supports v{} — \
             please update Arbor",
            manifest.name, manifest.arbor_api, ARBOR_API_VERSION
        )));
    }

    // Minimum Arbor app-version check (semver — author-facing constraint).
    if let Some(req_str) = manifest.min_arbor_version.as_deref() {
        let req_str = req_str.trim();
        if !req_str.is_empty() {
            match (
                semver::VersionReq::parse(req_str)
                    .or_else(|_| semver::VersionReq::parse(&format!(">={req_str}"))),
                semver::Version::parse(ARBOR_APP_VERSION),
            ) {
                (Ok(req), Ok(current)) if !req.matches(&current) => {
                    return Err(AppError::Plugin(format!(
                        "plugin '{}' requires Arbor {} but this build is {}",
                        manifest.name, req_str, ARBOR_APP_VERSION
                    )));
                }
                (Err(e), _) => {
                    tracing::warn!(
                        "plugin '{}': malformed min_arbor_version '{}' — ignoring ({})",
                        manifest.name, req_str, e
                    );
                }
                _ => {} // satisfied or current version unparseable (dev build)
            }
        }
    }

    let timer_cancels: TimerCancels = Arc::new(Mutex::new(HashMap::new()));
    let timer_counter: TimerCounter = Arc::new(AtomicU64::new(0));
    let schedules:     ScheduleRegistry = Arc::new(Mutex::new(Vec::new()));
    // Live enable flag — shared with closures inside the sandbox (api.rs)
    // so they can short-circuit even when the plugin gets disabled while a
    // background timer or scheduler tick is mid-call.
    let enabled: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));

    let sandbox_started = std::time::Instant::now();
    let lua = crate::plugin::sandbox::create_sandbox(
        &manifest,
        app_handle,
        timer_cancels.clone(),
        timer_counter,
        schedules.clone(),
        contributions,
        tree_store,
        icon_registry,
        enabled.clone(),
    )?;
    let sandbox_ms = sandbox_started.elapsed().as_millis();

    // Load the entry point.
    let exec_started = std::time::Instant::now();
    let entry_path = manifest.dir.join(&manifest.entry);
    if entry_path.exists() {
        let code = std::fs::read_to_string(&entry_path)?;
        lua.load(&code)
            .set_name(manifest.name.clone())
            .exec()
            .map_err(|e| AppError::Plugin(format!(
                "plugin '{}' load error: {e}", manifest.name
            )))?;
    }
    let exec_ms = exec_started.elapsed().as_millis();

    // Fire on_plugin_load — serves as the plugin constructor.
    let hook_started = std::time::Instant::now();
    let ctx = serde_json::json!({
        "plugin_name": manifest.name,
        "dir":         manifest.dir.to_string_lossy(),
        "api_version": manifest.arbor_api,
    });
    let _ = crate::plugin::hook_registry::fire(
        &lua, "on_plugin_load", &ctx.to_string(),
    );
    let hook_ms = hook_started.elapsed().as_millis();

    // Sub-phase breakdown — invaluable when one plugin dominates the boot
    // time. Logged at debug so it doesn't spam info-level builds.
    tracing::debug!(
        "plugin '{}' load breakdown: sandbox={}ms exec={}ms on_plugin_load={}ms",
        manifest.name, sandbox_ms, exec_ms, hook_ms,
    );
    // Anything single-phase >500ms is unusual and worth flagging at info,
    // even when the overall plugin load is acceptable.
    if sandbox_ms > 500 || exec_ms > 500 || hook_ms > 500 {
        tracing::info!(
            "plugin '{}' SLOW phase: sandbox={}ms exec={}ms on_plugin_load={}ms",
            manifest.name, sandbox_ms, exec_ms, hook_ms,
        );
    }

    Ok(LoadedPlugin {
        manifest,
        lua,
        enabled,
        timer_cancels,
        schedules,
    })
}
