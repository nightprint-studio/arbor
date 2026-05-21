//! `PluginHost` — central registry of every plugin Arbor knows about.
//!
//! The struct itself plus the small lifecycle primitives (`new`,
//! `set_app_handle`, `unload_all`) live here. Larger groups of methods are
//! split into sibling modules: `lifecycle` (load/enable/disable/delete),
//! `hooks`, `service`, `pipeline_op`, `introspection`. The scheduler thread
//! spawn helper is kept in `super::scheduler` next to its loop function.

pub mod hooks;
pub mod introspection;
pub mod lifecycle;
pub mod pipeline_op;
pub mod service;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::loaded::{DormantPlugin, LoadedPlugin};
use super::manifest::deps::PluginLoadFailure;

pub struct PluginHost {
    pub plugins:    Vec<LoadedPlugin>,
    /// Plugins that exist on disk + pass dependency resolution but were
    /// disabled in `plugin_states.json` at startup, so we never spun up a
    /// Lua VM for them. Re-enabling moves them into `plugins` via a fresh
    /// `load_plugin()` call. Surfaced in the Plugin Manager so the user
    /// can flip them back on.
    pub dormant:    Vec<DormantPlugin>,
    pub(crate) app_handle: Option<tauri::AppHandle>,
    /// Cancel tokens keyed by "<plugin_name>:<schedule_action>" for schedulers.
    pub(crate) scheduler_cancels: HashMap<String, Arc<AtomicBool>>,
    /// Plugins that failed to load due to dependency errors (shown in Plugin Manager).
    pub load_failures: Vec<PluginLoadFailure>,
    /// Cross-plugin contribution registry (arbor.ui.contribute).
    pub contributions: crate::plugin::contribution::ContributionRegistry,
    /// Tree-state storage for kind="tree" sidebars (arbor.ui.tree.set).
    pub tree_store:    crate::plugin::tree::TreeStore,
    /// Plugin-supplied custom SVG icons (arbor.ui.icon.register).
    pub icon_registry: crate::plugin::tree::IconRegistry,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins:           Vec::new(),
            dormant:           Vec::new(),
            app_handle:        None,
            scheduler_cancels: HashMap::new(),
            load_failures:     Vec::new(),
            contributions:     crate::plugin::contribution::ContributionRegistry::new(),
            tree_store:        crate::plugin::tree::TreeStore::new(),
            icon_registry:     crate::plugin::tree::IconRegistry::new(),
        }
    }

    pub fn set_app_handle(&mut self, handle: tauri::AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Tear down every loaded plugin without re-discovering anything from
    /// disk. Used by `reload()` (before re-loading) and by the master plugin
    /// kill-switch when the user toggles the system off in the Plugin Manager.
    pub fn unload_all(&mut self) {
        // Cancel all existing schedulers and Lua timers.
        for cancel in self.scheduler_cancels.values() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.scheduler_cancels.clear();

        // Fire on_plugin_unload on all currently loaded (enabled) plugins.
        for plugin in &self.plugins {
            if plugin.is_enabled() {
                let _ = crate::plugin::hook_registry::fire(
                    &plugin.lua, "on_plugin_unload", "{}",
                );
            }
            // Cancel all Lua timers.
            if let Ok(tc) = plugin.timer_cancels.lock() {
                for cancel in tc.values() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }

        // Wipe cross-plugin shared state so contributions, tree snapshots, and
        // custom icons from the previous incarnation don't outlive their authors.
        for plugin in &self.plugins {
            self.contributions.remove_plugin(&plugin.manifest.name);
            self.tree_store.remove_plugin(&plugin.manifest.name);
            self.icon_registry.remove_plugin(&plugin.manifest.name);
        }

        self.plugins.clear();
        self.dormant.clear();
        self.load_failures.clear();
    }
}
