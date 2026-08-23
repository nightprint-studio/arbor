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

use crate::error::{PluginCoreError, Result};

use super::consts::current_os;

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Every root a profile's plugins live in, in precedence order.
///
/// **Two pools, and both of them always.** [`plugin_dir`] is the host pool — the profile's
/// `installed/`, or the workspace's `plugins/` in a debug build. The marketplace pool is the
/// other, and on a real installation it holds essentially everything the user has.
///
/// This used to be a per-call-site decision, and every site that took only the first pool was
/// silently wrong: in a debug build both resolve to something populated, so the mistake is
/// invisible until a release build, where it becomes "the Plugin Manager is empty" or "no
/// extension provides mesh-source@1/primitives" — a missing root wearing a missing-package
/// error message.
///
/// Computed per call, not cached: the marketplace root is per **profile**, and a live profile
/// switch has to change the answer without anybody re-registering anything.
pub fn plugin_roots() -> Vec<PathBuf> {
    vec![plugin_dir(), arbor_core::prelude::marketplace_plugins_dir()]
}

/// Every plugin the active profile has, across both [`plugin_roots`].
pub fn discover_plugins() -> Result<Vec<Manifest>> {
    Ok(discover_in_roots(&plugin_roots())?.0)
}

/// Same as `discover_plugins`, but caller-supplied roots. The host shell
/// crate composes its own slice (host `plugin_dir()` + marketplace install
/// dir) and feeds it in so this crate stays free of any marketplace coupling.
///
/// Roots are scanned in order; later roots cannot shadow names that earlier
/// roots already claimed — a name collision is logged and the later entry is
/// skipped. The second return value is the list of folders whose manifest
/// failed to parse (caller uses it to render "broken plugin" entries in the
/// Plugin Manager).
pub fn discover_in_roots(
    roots: &[PathBuf],
) -> Result<(Vec<Manifest>, Vec<ManifestParseFailure>)> {
    let host_os = current_os();
    let mut manifests: Vec<Manifest> = Vec::new();
    let mut bad: Vec<ManifestParseFailure> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for dir in roots {
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
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(manifest)
}

pub fn plugin_dir() -> PathBuf {
    // In dev (debug) builds, load plugins from the workspace's `plugins/`
    // directory so we don't fight with whatever's installed under
    // `~/.config/arbor/plugins` for a stable Arbor running in parallel.
    // CARGO_MANIFEST_DIR is replaced at compile time with the absolute path
    // of `crates/platform/plugin/core/`, so we walk up four levels to the
    // workspace root (`core` → `plugin` → `platform` → `crates` → workspace).
    if cfg!(debug_assertions) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(workspace) = manifest_dir.ancestors().nth(4) {
            let dev_plugins = workspace.join("plugins");
            if dev_plugins.exists() {
                return dev_plugins;
            }
        }
    }
    arbor_core::prelude::profile_plugins_dir().join("installed")
}

// ---------------------------------------------------------------------------
// Topological sort (Kahn's algorithm) — orders manifests so that every plugin
// is loaded after the plugins it depends on. Plugins that participate in a
// cycle are returned separately so the caller can flag them as errors.
// ---------------------------------------------------------------------------

pub fn topo_sort_manifests(
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
//
// ## Why this is keyed by product
//
// The first version was a flat `{ name: enabled }` — one decision for the whole application.
// That conflated two facts that are not the same: whether a package is **on disk** (one copy,
// downloaded once) and whether the user wants it **here**. Installing a git plugin from Corvus
// put it in Bennu's command palette too, and uninstalling it to get rid of it there threw away
// the download.
//
// Now the bytes stay global and the decision is per product. Installing from one window writes
// "yes" for that product and an explicit "no" for the others, so a later install elsewhere is a
// flag flip rather than a second download.
//
// ## Three states, not two
//
// `true` / `false` / **absent**, and absent is load-bearing. A package nobody has ever decided
// about — a folder dropped into the dev `plugins/` directory, which is how every plugin in this
// repo runs — has no entry anywhere, and must still load. So absent means *yes, wherever its
// manifest allows*, and only an explicit `false` keeps it out.
//
// That is also what makes the migration from v1 free: the old flat map is kept as a fallback
// rather than expanded across products, so nothing that worked stops working, and it shrinks
// naturally as the user touches things.

use std::collections::BTreeMap;

fn plugin_states_path() -> PathBuf {
    // Per-profile; debug/release isolation comes from the active profile
    // (`dev` vs `default`), not a filename suffix.
    arbor_core::prelude::profile_plugins_dir().join("plugin_states.json")
}

/// Which products each package is installed for.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginStates {
    /// `product -> package -> installed here`.
    #[serde(default)]
    pub products: BTreeMap<String, BTreeMap<String, bool>>,
    /// The v1 flat map, consulted when a product has no opinion of its own.
    ///
    /// Kept rather than migrated across every product because expanding it would freeze
    /// today's answer for products that do not exist yet — a package enabled before Bennu
    /// hosted plugins would arrive there marked "installed" by a decision nobody made.
    #[serde(default)]
    pub legacy: BTreeMap<String, bool>,
}

impl PluginStates {
    /// Whether `plugin` should load under `product`.
    ///
    /// `None` for a host with no product (the shell's own in-process host) asks the same
    /// question of the legacy map alone, which is where its answer lived before.
    pub fn is_enabled(&self, product: Option<&str>, plugin: &str) -> bool {
        if let Some(p) = product {
            if let Some(explicit) = self.products.get(p).and_then(|m| m.get(plugin)) {
                return *explicit;
            }
        }
        // No per-product decision: the old global one, and failing that, yes. See the module
        // note — a package nobody has decided about is one somebody dropped in a folder.
        self.legacy.get(plugin).copied().unwrap_or(true)
    }

    /// Every package that is enabled for at least one product.
    ///
    /// What the **extension** index asks: an extension is invoked by a plugin that has already
    /// been product-filtered, so scoping it a second time would refuse a call the caller was
    /// entitled to make.
    pub fn enabled_anywhere(&self) -> HashMap<String, bool> {
        let mut out: HashMap<String, bool> = HashMap::new();
        for names in self.products.values() {
            for (name, on) in names {
                *out.entry(name.clone()).or_insert(false) |= *on;
            }
        }
        for (name, on) in &self.legacy {
            out.entry(name.clone()).or_insert(*on);
        }
        out
    }

    /// Record that `plugin` belongs to `product` and to no other.
    ///
    /// The explicit `false` for the others is the whole mechanism: without it, absent would
    /// mean "yes" everywhere and an install would still be an install everywhere.
    pub fn install_for(&mut self, product: &str, plugin: &str) {
        for p in arbor_plugin_types::prelude::HOSTING_PRODUCTS {
            self.products
                .entry((*p).to_string())
                .or_default()
                .insert(plugin.to_string(), *p == product);
        }
        // The old global entry would otherwise keep answering for a product added later.
        self.legacy.remove(plugin);
    }

    /// Give a package a product without changing whether it runs.
    ///
    /// The two-step this replaces is a trap: [`Self::install_for`] marks the product `true`,
    /// so using it to scope a package the user had switched **off** would switch it back on
    /// as a side effect of tidying the file.
    pub fn scope_to(&mut self, product: &str, plugin: &str) {
        let was_enabled = self.is_enabled(None, plugin);
        self.install_for(product, plugin);
        self.set(product, plugin, was_enabled);
    }

    /// Turn `plugin` on or off for one product, leaving every other product alone.
    pub fn set(&mut self, product: &str, plugin: &str, enabled: bool) {
        self.products
            .entry(product.to_string())
            .or_default()
            .insert(plugin.to_string(), enabled);
    }

    /// Forget a package entirely — what uninstalling from disk means.
    pub fn forget(&mut self, plugin: &str) {
        for names in self.products.values_mut() {
            names.remove(plugin);
        }
        self.legacy.remove(plugin);
    }

    /// The products this package is installed for, for the Plugin Manager to show.
    pub fn products_of(&self, plugin: &str) -> Vec<String> {
        arbor_plugin_types::prelude::HOSTING_PRODUCTS
            .iter()
            .filter(|p| self.is_enabled(Some(p), plugin))
            .map(|p| (*p).to_string())
            .collect()
    }
}

/// Read the states file, migrating a v1 flat map on the way.
pub fn load_states() -> PluginStates {
    let path = plugin_states_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return PluginStates::default();
    };
    // v2 first. A v1 file is a flat `{ "name": bool }`, which does not have `products` or
    // `legacy` — and `#[serde(default)]` would happily accept it as an EMPTY v2, silently
    // dropping every recorded decision. So the flat shape is tried first.
    if let Ok(flat) = serde_json::from_str::<BTreeMap<String, bool>>(&text) {
        return PluginStates { products: BTreeMap::new(), legacy: flat };
    }
    serde_json::from_str::<PluginStates>(&text).unwrap_or_default()
}

pub fn save_states(states: &PluginStates) {
    let path = plugin_states_path();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(json) = serde_json::to_string_pretty(states) {
        let _ = std::fs::write(&path, json);
    }
}

/// The enabled map as one product sees it.
///
/// Materialised for the callers that already hold a `HashMap<String, bool>` — the answer is
/// the same one [`PluginStates::is_enabled`] gives, for the packages that have an entry.
pub fn load_plugin_states_for(product: Option<&str>) -> HashMap<String, bool> {
    let states = load_states();
    let mut out: HashMap<String, bool> = HashMap::new();
    for (name, on) in &states.legacy {
        out.insert(name.clone(), *on);
    }
    if let Some(p) = product {
        if let Some(names) = states.products.get(p) {
            for (name, on) in names {
                out.insert(name.clone(), *on);
            }
        }
    }
    out
}

/// The v1 entry point, kept for callers with no product in hand.
///
/// Answers with what is enabled **anywhere**, which is right for the extension index and wrong
/// for a plugin host — hosts pass their product to [`load_plugin_states_for`].
pub fn load_plugin_states() -> HashMap<String, bool> {
    load_states().enabled_anywhere()
}

/// Write a flat map back, as a per-product one.
///
/// Only the entries that changed are touched, so a caller holding a stale map cannot wipe a
/// decision made in another window since it read.
pub fn save_plugin_states_for(product: Option<&str>, map: &HashMap<String, bool>) {
    let mut states = load_states();
    match product {
        Some(p) => {
            for (name, on) in map {
                states.set(p, name, *on);
            }
        }
        None => {
            for (name, on) in map {
                states.legacy.insert(name.clone(), *on);
            }
        }
    }
    save_states(&states);
}

pub fn save_plugin_states(map: &HashMap<String, bool>) {
    save_plugin_states_for(None, map);
}

/// Turn one package on or off for one product.
///
/// Read-modify-write against the file rather than against a map the caller is holding: two
/// windows toggling different plugins at the same time must not overwrite each other.
pub fn set_plugin_state_for(product: Option<&str>, plugin: &str, enabled: bool) {
    let mut states = load_states();
    match product {
        Some(p) => states.set(p, plugin, enabled),
        None => {
            states.legacy.insert(plugin.to_string(), enabled);
        }
    }
    save_states(&states);
}

/// Record that a package now belongs to one product and to no other. What installing means.
pub fn install_plugin_for(product: &str, plugin: &str) {
    let mut states = load_states();
    states.install_for(product, plugin);
    save_states(&states);
}

/// Drop every decision about a package. What uninstalling from disk means.
pub fn forget_plugin_state(plugin: &str) {
    let mut states = load_states();
    states.forget(plugin);
    save_states(&states);
}

/// Which products a package is installed for.
pub fn plugin_products(plugin: &str) -> Vec<String> {
    load_states().products_of(plugin)
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn a_package_nobody_decided_about_loads_everywhere() {
        // How every plugin in the dev `plugins/` folder runs: no entry anywhere.
        let s = PluginStates::default();
        assert!(s.is_enabled(Some("corvus"), "dropped-in"));
        assert!(s.is_enabled(Some("bennu"), "dropped-in"));
        assert!(s.is_enabled(None, "dropped-in"));
    }

    #[test]
    fn installing_for_one_product_does_not_install_it_for_another() {
        // The requirement, in one assertion.
        let mut s = PluginStates::default();
        s.install_for("corvus", "git-thing");
        assert!(s.is_enabled(Some("corvus"), "git-thing"));
        assert!(!s.is_enabled(Some("bennu"), "git-thing"));
        assert!(!s.is_enabled(Some("merula"), "git-thing"));
    }

    #[test]
    fn a_second_install_elsewhere_is_a_flag_flip() {
        // "se è già installato localmente, non serve che lo riscarico" — nothing here touches
        // the disk, which is the point: the bytes are global, the decision is not.
        let mut s = PluginStates::default();
        s.install_for("corvus", "thing");
        s.set("bennu", "thing", true);
        assert!(s.is_enabled(Some("corvus"), "thing"));
        assert!(s.is_enabled(Some("bennu"), "thing"));
        assert_eq!(s.products_of("thing"), vec!["corvus", "bennu"]);
    }

    #[test]
    fn a_v1_map_keeps_working_everywhere_it_used_to() {
        // The migration must not switch anything off: a package enabled before products
        // existed was enabled for the whole app.
        let s = PluginStates {
            products: BTreeMap::new(),
            legacy: BTreeMap::from([
                ("on".to_string(), true),
                ("off".to_string(), false),
            ]),
        };
        assert!(s.is_enabled(Some("corvus"), "on"));
        assert!(s.is_enabled(Some("bennu"), "on"));
        assert!(!s.is_enabled(Some("corvus"), "off"));
    }

    #[test]
    fn a_per_product_decision_beats_the_old_global_one() {
        let mut s = PluginStates {
            products: BTreeMap::new(),
            legacy: BTreeMap::from([("thing".to_string(), true)]),
        };
        s.set("bennu", "thing", false);
        assert!(s.is_enabled(Some("corvus"), "thing"), "corvus never objected");
        assert!(!s.is_enabled(Some("bennu"), "thing"));
    }

    #[test]
    fn installing_clears_the_old_global_entry() {
        // Left behind, it would answer for a product added after the install and quietly
        // undo the scoping.
        let mut s = PluginStates {
            products: BTreeMap::new(),
            legacy: BTreeMap::from([("thing".to_string(), true)]),
        };
        s.install_for("corvus", "thing");
        assert!(!s.legacy.contains_key("thing"));
        assert!(!s.is_enabled(Some("bennu"), "thing"));
    }

    #[test]
    fn scoping_a_package_keeps_whether_it_was_switched_off() {
        // Giving an existing install a product is tidying, and tidying must not turn anything
        // back on: `install_for` alone marks the product `true`.
        let mut s = PluginStates {
            products: BTreeMap::new(),
            legacy: BTreeMap::from([("off".to_string(), false), ("on".to_string(), true)]),
        };
        s.scope_to("corvus", "off");
        s.scope_to("corvus", "on");
        assert!(!s.is_enabled(Some("corvus"), "off"), "a disabled package came back on");
        assert!(s.is_enabled(Some("corvus"), "on"));
        // And in both cases it is now absent from everywhere else, which is the point.
        assert!(!s.is_enabled(Some("bennu"), "on"));
    }

    #[test]
    fn an_extension_is_reachable_when_any_product_has_it() {
        // An extension is called by a plugin that was already product-filtered; scoping it
        // again would refuse a call the caller was entitled to make.
        let mut s = PluginStates::default();
        s.install_for("bennu", "meshes");
        let anywhere = s.enabled_anywhere();
        assert_eq!(anywhere.get("meshes"), Some(&true));
    }

    #[test]
    fn forgetting_a_package_removes_every_trace() {
        let mut s = PluginStates::default();
        s.install_for("corvus", "thing");
        s.legacy.insert("thing".into(), true);
        s.forget("thing");
        assert!(s.products.values().all(|m| !m.contains_key("thing")));
        assert!(!s.legacy.contains_key("thing"));
    }
}
