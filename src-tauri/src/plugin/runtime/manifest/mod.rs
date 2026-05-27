//! Plugin discovery, dependency topological sort, and the per-user persisted
//! enable/disable state file.
//!
//! The pure-data manifest types (`Manifest`, `Permissions`, `Dependency`,
//! `Hooks`, `Sandbox`, `Schedule*`) live in the `arbor-plugin-types` crate.
//! This module keeps the host-side I/O: walking the plugin directories,
//! reading + parsing `plugin.toml` files off disk, ordering them by
//! dependency, and round-tripping the user's enable/disable state through
//! `~/.config/arbor/plugin_states.json`.

pub mod info;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use arbor_plugin_types::prelude::{Manifest, ManifestParseFailure};

use crate::error::Result;

use super::consts::current_os;

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

pub fn discover_plugins() -> Result<Vec<Manifest>> {
    Ok(discover_plugins_detailed()?.0)
}

/// Same as `discover_plugins`, but also returns the list of folders whose
/// manifest failed to parse. The caller (PluginHost::reload) uses it to
/// surface those failures in the Plugin Logs panel and the Plugin Manager.
pub fn discover_plugins_detailed() -> Result<(Vec<Manifest>, Vec<ManifestParseFailure>)> {
    let host_os = current_os();
    let mut manifests: Vec<Manifest> = Vec::new();
    let mut bad: Vec<ManifestParseFailure> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    // Two roots, scanned in order so dev / hand-installed plugins win on
    // name collision with marketplace installs (marketplace shadow is
    // skipped + warned in the logs).
    //
    //   1. `plugin_dir()`            — dev / manual installs (workspace
    //      `plugins/` in debug, `~/.config/arbor/plugins` in release).
    //   2. `marketplace_plugin_dir()` — downloads from the marketplace.
    let roots = [plugin_dir(), crate::marketplace::installs::marketplace_plugin_dir()];

    for dir in &roots {
        if !dir.exists() { continue; }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path  = entry.path();
            if !path.is_dir() { continue; }
            let toml_path = path.join("plugin.toml");
            if !toml_path.exists() { continue; }
            match read_manifest(&toml_path, &path) {
                Ok(m) => {
                    if !m.os.is_empty() && !m.os.iter().any(|o| o == host_os) {
                        tracing::info!(
                            "plugin '{}' skipped: os={:?} does not include host '{}'",
                            m.name, m.os, host_os
                        );
                        continue;
                    }
                    if !seen_names.insert(m.name.clone()) {
                        tracing::warn!(
                            "plugin '{}' shadowed: an entry from an earlier root already \
                             claimed this name, skipping {:?}",
                            m.name, path
                        );
                        continue;
                    }
                    manifests.push(m);
                }
                Err(e) => {
                    tracing::warn!("bad manifest at {toml_path:?}: {e}");
                    let folder_name = path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("<unknown>")
                        .to_string();
                    bad.push(ManifestParseFailure {
                        folder_name,
                        error: e.to_string(),
                    });
                }
            }
        }
    }
    Ok((manifests, bad))
}

fn read_manifest(toml_path: &std::path::Path, dir: &std::path::Path) -> Result<Manifest> {
    let content = std::fs::read_to_string(toml_path)?;
    let manifest = Manifest::from_toml_str(&content, dir)
        .map_err(|e| crate::error::AppError::Plugin(e.to_string()))?;
    Ok(manifest)
}

pub fn plugin_dir() -> PathBuf {
    // In dev (debug) builds, load plugins from the workspace's `plugins/`
    // directory so we don't fight with whatever's installed under
    // `~/.config/arbor/plugins` for a stable Arbor running in parallel.
    // CARGO_MANIFEST_DIR is replaced at compile time with the absolute path
    // of `src-tauri/`, so we go up one level to the workspace root.
    if cfg!(debug_assertions) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(workspace) = manifest_dir.parent() {
            let dev_plugins = workspace.join("plugins");
            if dev_plugins.exists() {
                return dev_plugins;
            }
        }
    }
    arbor_core::prelude::arbor_config_path("plugins")
}

// ---------------------------------------------------------------------------
// Topological sort (Kahn's algorithm) — orders manifests so that every plugin
// is loaded after the plugins it depends on. Plugins that participate in a
// cycle are returned separately so the caller can flag them as errors.
// ---------------------------------------------------------------------------

pub(crate) fn topo_sort_manifests(
    manifests: Vec<Manifest>,
) -> (Vec<Manifest>, Vec<String>) {
    let known: HashSet<String> = manifests.iter().map(|m| m.name.clone()).collect();

    // Build adjacency: dep_name → [dependents]. Edges that point at plugins
    // that are not installed are ignored — the per-manifest check later will
    // emit a proper "dependency X not found" error for those.
    let mut indegree: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_name: HashMap<String, Manifest> = HashMap::new();
    for m in &manifests {
        indegree.entry(m.name.clone()).or_insert(0);
        for d in &m.dependencies {
            if known.contains(&d.name) {
                *indegree.entry(m.name.clone()).or_insert(0) += 1;
                adj.entry(d.name.clone()).or_default().push(m.name.clone());
            }
        }
    }
    for m in manifests {
        by_name.insert(m.name.clone(), m);
    }

    // Queue starts with every plugin whose in-degree is zero.
    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(k, _)| k.clone())
        .collect();

    let mut sorted: Vec<Manifest> = Vec::with_capacity(by_name.len());
    while let Some(name) = queue.pop_front() {
        if let Some(m) = by_name.remove(&name) {
            sorted.push(m);
        }
        if let Some(children) = adj.get(&name) {
            for child in children {
                if let Some(deg) = indegree.get_mut(child) {
                    if *deg > 0 { *deg -= 1; }
                    if *deg == 0 { queue.push_back(child.clone()); }
                }
            }
        }
    }

    // Anything left in by_name is in a cycle.
    let cycle_names: Vec<String> = by_name.keys().cloned().collect();
    (sorted, cycle_names)
}

// ---------------------------------------------------------------------------
// Persisted enabled-state helpers
// ---------------------------------------------------------------------------

fn plugin_states_path() -> PathBuf {
    // In dev (debug) builds, use a separate filename so dev sessions don't
    // overwrite the prod-installed Arbor's enable/disable state.
    let filename = if cfg!(debug_assertions) {
        "plugin_states-dev.json"
    } else {
        "plugin_states.json"
    };
    arbor_core::prelude::arbor_config_path(filename)
}

pub(crate) fn load_plugin_states() -> HashMap<String, bool> {
    let path = plugin_states_path();
    if !path.exists() { return HashMap::new(); }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_plugin_states(map: &HashMap<String, bool>) {
    let path = plugin_states_path();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(&path, json);
    }
}
