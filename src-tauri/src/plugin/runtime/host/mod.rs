//! `PluginHost` — central registry of every plugin Arbor knows about.
//!
//! The struct itself plus the small lifecycle primitives (`new`,
//! `set_app_handle`, `unload_all`) live here. Larger groups of methods are
//! split into sibling modules: `lifecycle` (load/enable/disable/delete),
//! `hooks`, `service`, `pipeline_op`, `introspection`. The scheduler thread
//! spawn helper is kept in `super::scheduler` next to its loop function.

pub mod dep_cascade;
pub mod hooks;
pub mod introspection;
pub mod lifecycle;
pub mod pipeline_op;
pub mod service;

use std::sync::{Arc, Mutex, Weak};

use arbor_scheduler::prelude::Scheduler;

use arbor_plugin_types::prelude::LoadFailure;

use super::loaded::{DormantPlugin, LoadedPlugin};

pub struct PluginHost {
    pub plugins:    Vec<LoadedPlugin>,
    /// Plugins that exist on disk + pass dependency resolution but were
    /// disabled in `plugin_states.json` at startup, so we never spun up a
    /// Lua VM for them. Re-enabling moves them into `plugins` via a fresh
    /// `load_plugin()` call. Surfaced in the Plugin Manager so the user
    /// can flip them back on.
    pub dormant:    Vec<DormantPlugin>,
    pub(crate) app_handle: Option<tauri::AppHandle>,
    /// Shared trigger engine. Set once at boot via [`set_scheduler`] (after
    /// `setup()` has constructed it on the running Tokio runtime). `None`
    /// means "scheduling disabled" — plugin lifecycle code that needs to
    /// register / cancel schedules treats `None` as a no-op rather than
    /// panicking.
    pub(crate) scheduler: Option<Arc<Scheduler>>,
    /// Weak self-reference, set alongside [`set_scheduler`]. Lua-bridge
    /// actions installed in the engine upgrade this to call back into
    /// `fire_hook_on`; using `Weak` avoids a self-strong-cycle.
    pub(crate) self_arc: Option<Weak<Mutex<PluginHost>>>,
    /// Plugins that failed to load due to dependency errors (shown in Plugin Manager).
    pub load_failures: Vec<LoadFailure>,
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
            scheduler:         None,
            self_arc:          None,
            load_failures:     Vec::new(),
            contributions:     crate::plugin::contribution::ContributionRegistry::new(),
            tree_store:        crate::plugin::tree::TreeStore::new(),
            icon_registry:     crate::plugin::tree::IconRegistry::new(),
        }
    }

    pub fn set_app_handle(&mut self, handle: tauri::AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Wire the shared trigger engine + the weak self-pointer used by
    /// scheduler-fired Lua actions to call back into [`Self::fire_hook_on`].
    /// Called once at boot from `setup()` after `AppState` is `manage`d
    /// and the Tokio runtime handle has been captured.
    pub fn install_scheduler(
        &mut self,
        scheduler: Arc<Scheduler>,
        self_arc:  Weak<Mutex<PluginHost>>,
    ) {
        self.scheduler = Some(scheduler);
        self.self_arc  = Some(self_arc);
    }

    /// Tear down every loaded plugin without re-discovering anything from
    /// disk. Used by `reload()` (before re-loading) and by the master plugin
    /// kill-switch when the user toggles the system off in the Plugin Manager.
    pub fn unload_all(&mut self) {
        // Cancel every scheduled entry that any plugin owns. The shared
        // engine cancels by namespace so we don't have to walk plugin-by-plugin.
        if let Some(sched) = &self.scheduler {
            for plugin in &self.plugins {
                sched.cancel_namespace(
                    &super::scheduler::plugin_namespace(&plugin.manifest.name),
                );
            }
        }

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
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
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
