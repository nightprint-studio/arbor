//! `PluginHost` — central registry of every plugin Arbor knows about.
//!
//! The struct itself plus the small lifecycle primitives (`new`,
//! `set_app_ctx`, `unload_all`) live here. Larger groups of methods are
//! split into sibling modules: `lifecycle` (load/enable/disable/delete),
//! `hooks`, `service`, `pipeline_op`, `introspection`. The scheduler thread
//! spawn helper is kept in `super::scheduler` next to its loop function.

pub mod command;
pub mod dep_cascade;
pub mod hooks;
pub mod introspection;
pub mod lifecycle;
pub mod pipeline_op;
pub mod service;

use std::sync::{Arc, Mutex, Weak};

use arbor_core::prelude::AppCtx;
use arbor_scheduler::prelude::Scheduler;

use arbor_plugin_types::prelude::{hook_names, LoadFailure};

use super::loaded::{DormantPlugin, LoadedPlugin, PluginActivity, ServiceIndex};
use crate::contribution::ContributionRegistry;
use crate::sandbox::LuaApiInstaller;
use crate::tree::{IconRegistry, TreeStore};

pub struct PluginHost {
    pub plugins:    Vec<LoadedPlugin>,
    /// Plugins that exist on disk + pass dependency resolution but were
    /// disabled in `plugin_states.json` at startup, so we never spun up a
    /// Lua VM for them. Re-enabling moves them into `plugins` via a fresh
    /// `load_plugin()` call. Surfaced in the Plugin Manager so the user
    /// can flip them back on.
    pub dormant:    Vec<DormantPlugin>,
    /// Host context handle. Used to record plugin log entries, emit frontend
    /// events, and locate the Arbor data root. `None` between host
    /// construction and [`set_app_ctx`].
    pub(crate) app_ctx: Option<Arc<dyn AppCtx>>,
    /// Installer that publishes the `arbor.*` Lua surface into every
    /// freshly-built sandbox VM. The host shell crate creates one at boot
    /// (production), or `NoopApiInstaller` is used in tests / headless runs.
    pub(crate) api_installer: Option<Arc<dyn LuaApiInstaller>>,
    /// Extra plugin roots (besides the host's `plugin_dir()`) to walk during
    /// `discover_plugins_detailed`. Set by the host shell so marketplace
    /// installs can live in a separate directory without coupling this crate
    /// to the marketplace module.
    pub(crate) extra_plugin_roots: Vec<std::path::PathBuf>,
    /// Shared trigger engine. Set once at boot via [`install_scheduler`]
    /// (after `setup()` has constructed it on the running Tokio runtime).
    /// `None` means "scheduling disabled" — plugin lifecycle code that needs
    /// to register / cancel schedules treats `None` as a no-op rather than
    /// panicking.
    pub(crate) scheduler: Option<Arc<Scheduler>>,
    /// Weak self-reference, set alongside [`install_scheduler`]. Lua-bridge
    /// actions installed in the engine upgrade this to call back into
    /// `hook_router::fire_on`; using `Weak` avoids a self-strong-cycle.
    pub(crate) self_arc: Option<Weak<Mutex<PluginHost>>>,
    /// The product whose backend this host serves (`"corvus"`, `"merula"`, …),
    /// used to filter plugins by their manifest `targets`. `None` means
    /// "load every plugin regardless of target" — the legacy / test default.
    /// Set once at boot via [`set_product`].
    pub(crate) product: Option<String>,
    /// Plugins that failed to load due to dependency errors (shown in Plugin Manager).
    pub load_failures: Vec<LoadFailure>,
    /// Cross-plugin contribution registry (arbor.ui.contribute).
    pub contributions: ContributionRegistry,
    /// Tree-state storage for kind="tree" sidebars (arbor.ui.tree.set).
    pub tree_store:    TreeStore,
    /// Plugin-supplied custom SVG icons (arbor.ui.icon.register).
    pub icon_registry: IconRegistry,
    /// Which plugins have a live VM, and whether each is enabled — the lock-free answer to
    /// `arbor.meta.plugin_loaded`. Maintained wherever `plugins` gains or loses an entry;
    /// enable/disable need no upkeep because the flags are the very `Arc<AtomicBool>`s the
    /// loaded plugins hold.
    pub activity: PluginActivity,
    /// Which cross-plugin services each loaded plugin exports — the lock-free answer to
    /// `arbor.service.list`. Written from inside the exporting plugin's VM; the host only
    /// retires entries when a VM goes away.
    pub services: ServiceIndex,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins:            Vec::new(),
            dormant:            Vec::new(),
            app_ctx:            None,
            api_installer:      None,
            extra_plugin_roots: Vec::new(),
            scheduler:          None,
            self_arc:           None,
            product:            None,
            load_failures:      Vec::new(),
            contributions:      ContributionRegistry::new(),
            tree_store:         TreeStore::new(),
            icon_registry:      IconRegistry::new(),
            activity:           PluginActivity::new(),
            services:           ServiceIndex::new(),
        }
    }

    /// Install the host context handle. Called once at boot from the host
    /// shell after the Tauri `AppHandle` is wrapped in `TauriAppCtx`. Also
    /// propagates the handle into [`ContributionRegistry`] so contributions
    /// emitted from this point on reach the frontend.
    pub fn set_app_ctx(&mut self, ctx: Arc<dyn AppCtx>) {
        self.contributions.install_app_ctx(ctx.clone());
        self.app_ctx = Some(ctx);
    }

    /// Install the Lua API installer. The shell crate creates the production
    /// installer (it depends on the Tauri-bound `arbor.*` namespace surface
    /// that hasn't migrated yet); tests can install
    /// [`NoopApiInstaller`](crate::sandbox::NoopApiInstaller).
    pub fn set_api_installer(&mut self, installer: Arc<dyn LuaApiInstaller>) {
        self.api_installer = Some(installer);
    }

    /// Bind this host to a product (`"corvus"`, `"merula"`, …) so that
    /// [`reload`](lifecycle) loads only the plugins whose manifest `targets`
    /// include this product (universal plugins — empty `targets` — always load).
    /// Called once at boot by the product backend / shell. Leaving it unset
    /// loads every plugin regardless of target.
    pub fn set_product(&mut self, product: impl Into<String>) {
        self.product = Some(product.into());
    }

    /// Set extra plugin roots (e.g. the marketplace install dir) that should
    /// be scanned in addition to the host's `plugin_dir()` during
    /// `discover_plugins_detailed`. Order matters — earlier roots win on
    /// name collisions.
    pub fn set_extra_plugin_roots(&mut self, roots: Vec<std::path::PathBuf>) {
        self.extra_plugin_roots = roots;
    }

    /// Snapshot the installed `AppCtx`, if any. Returns `None` in the brief
    /// window between `PluginHost::new` and the host shell calling
    /// [`set_app_ctx`].
    pub fn app_ctx(&self) -> Option<Arc<dyn AppCtx>> {
        self.app_ctx.clone()
    }

    /// Wire the shared trigger engine + the weak self-pointer used by
    /// scheduler-fired Lua actions to call back into
    /// [`crate::hook_router::fire_on`]. Called once at boot from `setup()`
    /// after `AppState` is `manage`d and the Tokio runtime handle is captured.
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
                let _ = crate::hook_router::fire(
                    &plugin.lua, hook_names::arbor::PLUGIN_UNLOAD, "{}",
                );
            }
            // Cancel all Lua timers.
            if let Ok(tc) = plugin.timer_cancels.lock() {
                for cancel in tc.values() {
                    cancel.cancel();
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
        self.activity.clear();
        self.services.clear();
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
