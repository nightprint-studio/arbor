//! Per-plugin live state held by the host: an active `LoadedPlugin` (with its
//! own Lua VM) or a `DormantPlugin` (manifest only — no VM until the user
//! enables it).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use arbor_plugin_types::prelude::{Manifest, ScheduleRegistry};

// ---------------------------------------------------------------------------
// Timer registry — tracks per-plugin timers spawned from Lua
// ---------------------------------------------------------------------------

/// Cancellation + interruptible-sleep primitive for `arbor.timer.*`.
///
/// Replaces a bare `AtomicBool` that timer threads used to busy-poll every
/// 50ms: that woke each timer thread 20×/second purely to re-check the flag,
/// kept the process from going quiescent while Arbor sat in the background
/// (worsening battery and the post-standby freeze), and added up to 50ms of
/// latency to every cancel. Here the timer thread parks on the condvar for the
/// whole interval and is woken instantly by [`cancel`](Self::cancel).
pub struct TimerCancel {
    cancelled: AtomicBool,
    lock:      Mutex<()>,
    cv:        Condvar,
}

impl TimerCancel {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            lock:      Mutex::new(()),
            cv:        Condvar::new(),
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Flag the timer as cancelled and wake any thread parked in
    /// [`sleep_or_cancel`](Self::sleep_or_cancel). Idempotent.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        // Hold the lock while notifying so a thread about to park can't miss
        // the wake — it re-checks the predicate under this same lock.
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.cv.notify_all();
    }

    /// Park for up to `dur`, returning early the instant `cancel()` fires.
    /// Returns `true` if cancelled (caller stops), `false` on a normal timeout
    /// (caller fires the hook). System sleep simply freezes the park, so it
    /// resumes the remaining interval on wake — never a backlog of fires.
    pub fn sleep_or_cancel(&self, dur: Duration) -> bool {
        if self.is_cancelled() {
            return true;
        }
        let guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .cv
            .wait_timeout_while(guard, dur, |_| !self.cancelled.load(Ordering::Relaxed))
            .unwrap_or_else(|e| e.into_inner());
        self.is_cancelled()
    }
}

pub type TimerCancels = Arc<Mutex<HashMap<String, Arc<TimerCancel>>>>;
pub type TimerCounter = Arc<AtomicU64>;

// ---------------------------------------------------------------------------
// Plugin activity — "which siblings are live right now", readable without the
// host mutex
// ---------------------------------------------------------------------------

/// Manifest name → that plugin's own live enable flag, for every plugin the host
/// currently has a VM for.
///
/// ## Why this exists instead of asking the host
///
/// `arbor.meta.plugin_loaded("x")` used to answer by locking the `Mutex<PluginHost>` and
/// calling `is_plugin_enabled`. That is correct from a background thread and a **deadlock**
/// from a hook: the host fires `arbor:plugin_load` while it still holds its own mutex, so a
/// plugin asking about a sibling from that hook re-entered a `std::sync::Mutex` on the same
/// thread and the whole backend stopped — no error, no timeout, every later RPC queued
/// behind it forever. `shader-preview` asks exactly that question, in exactly that hook.
///
/// The flags themselves are the same `Arc<AtomicBool>` the host and the plugin's own sandbox
/// already share, so nothing here is a second copy of the truth: enabling or disabling a
/// plugin flips one atomic and this map sees it without being touched. Only *loading* and
/// *unloading* change the map, which is why it can be maintained at four call sites.
///
/// The inner mutex is private and every method takes it, does one map operation and drops it.
/// Nothing that runs while it is held can reach back into the host, which is the property the
/// host's own mutex does not have.
#[derive(Clone, Default)]
pub struct PluginActivity(Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>);

impl PluginActivity {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Record a freshly loaded plugin. Replaces any previous entry, so a reload of the same
    /// name lands on the new VM's flag rather than the dead one's.
    pub fn publish(&self, name: &str, enabled: Arc<AtomicBool>) {
        if let Ok(mut g) = self.0.lock() {
            g.insert(name.to_string(), enabled);
        }
    }

    /// Drop a plugin the host no longer has a VM for. Disabling is **not** this — a disabled
    /// plugin keeps its entry and its flag simply reads false.
    pub fn retire(&self, name: &str) {
        if let Ok(mut g) = self.0.lock() {
            g.remove(name);
        }
    }

    /// Forget every plugin — the host unloaded them all.
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.clear();
        }
    }

    /// True when a plugin with that manifest name is loaded **and** enabled. False for
    /// unknown names, for dormant plugins (never published) and on a poisoned lock, so a
    /// caller can chain it as a soft check.
    pub fn is_enabled(&self, name: &str) -> bool {
        self.0
            .lock()
            .ok()
            .and_then(|g| g.get(name).map(|f| f.load(Ordering::Relaxed)))
            .unwrap_or(false)
    }
}

/// A plugin discovered on disk but kept in standby because its persisted
/// state is "disabled". No Lua VM, no `main.lua` execution, no schedulers —
/// promoting it to a `LoadedPlugin` happens only when the user explicitly
/// enables it.
pub struct DormantPlugin {
    pub manifest: Manifest,
}

pub struct LoadedPlugin {
    pub manifest:      Manifest,
    pub lua:           mlua::Lua,
    /// Live enable flag. Shared with closures inside the Lua VM (api.rs) so
    /// they can short-circuit even when fired from background threads after
    /// the user has just disabled the plugin. Always read via `is_enabled()`.
    pub enabled:       Arc<AtomicBool>,
    /// Cancel tokens for Lua-registered timers (arbor.timer.*).
    pub timer_cancels: TimerCancels,
    /// Schedules registered via `arbor.scheduler.register` from Lua. Shared
    /// with the API closure inside the sandbox so registrations land here.
    pub schedules:     ScheduleRegistry,
}

impl LoadedPlugin {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Service index — "which cross-plugin services exist", readable without the
// host mutex
// ---------------------------------------------------------------------------

/// Plugin name → the method names it currently exports through `arbor.service.export`.
///
/// The sibling of [`PluginActivity`], and it exists for the same reason: `arbor.service.list`
/// answered by locking the host and reflecting over every plugin's `__arbor_services__` Lua
/// table, which is the same re-entrant deadlock — a plugin listing what is available from its
/// own load hook would have hung the backend. The module header of `ns/service.rs` promised
/// that dispatch never blocks on the host mutex; `list` was the one function that broke it.
///
/// Written by the exporting plugin's own VM (`export` / `unexport`), retired by the host when
/// a plugin's VM goes away. Enablement is deliberately **not** tracked here — a disabled
/// plugin keeps its exports and [`qualified`](Self::qualified) filters on `PluginActivity`,
/// so the two registries answer one question each.
///
/// Ordered containers rather than hash ones: `arbor.service.list()` is a discovery call whose
/// output a plugin may show to a human, and an order that reshuffles between calls is worse
/// than one that is merely alphabetical.
#[derive(Clone, Default)]
pub struct ServiceIndex(Arc<Mutex<BTreeMap<String, BTreeSet<String>>>>);

impl ServiceIndex {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(BTreeMap::new())))
    }

    /// Record `plugin.method` as available.
    pub fn export(&self, plugin: &str, method: &str) {
        if let Ok(mut g) = self.0.lock() {
            g.entry(plugin.to_string()).or_default().insert(method.to_string());
        }
    }

    /// Withdraw one method. The plugin keeps its entry even when this empties it — it is
    /// still loaded, and it may export again.
    pub fn unexport(&self, plugin: &str, method: &str) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(methods) = g.get_mut(plugin) {
                methods.remove(method);
            }
        }
    }

    /// Drop a plugin whose VM is gone.
    pub fn retire(&self, plugin: &str) {
        if let Ok(mut g) = self.0.lock() {
            g.remove(plugin);
        }
    }

    /// Forget every plugin — the host unloaded them all.
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.clear();
        }
    }

    /// `"<plugin>.<method>"` for every service exported by a plugin that is **currently
    /// enabled**, alphabetically. A disabled plugin's exports are not callable, so listing
    /// them would advertise something `arbor.service.call` refuses.
    pub fn qualified(&self, activity: &PluginActivity) -> Vec<String> {
        let Ok(g) = self.0.lock() else { return Vec::new() };
        g.iter()
            .filter(|(plugin, _)| activity.is_enabled(plugin))
            .flat_map(|(plugin, methods)| methods.iter().map(move |m| format!("{plugin}.{m}")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(on: bool) -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(on))
    }

    #[test]
    fn an_unpublished_plugin_is_not_enabled() {
        let a = PluginActivity::new();
        // Dormant plugins are never published, and neither is a name nobody installed —
        // both have to read false rather than panic, because `plugin_loaded` is documented
        // as a soft check callers chain.
        assert!(!a.is_enabled("never-heard-of-it"));
    }

    #[test]
    fn disabling_a_plugin_needs_no_upkeep_here() {
        // The whole reason this holds `Arc<AtomicBool>` rather than `bool`: enable/disable
        // flips the flag the loaded plugin already shares, so the map cannot go stale
        // between a toggle and the next read.
        let a = PluginActivity::new();
        let live = flag(true);
        a.publish("cloud-storage", Arc::clone(&live));
        assert!(a.is_enabled("cloud-storage"));

        live.store(false, Ordering::Relaxed);
        assert!(!a.is_enabled("cloud-storage"));
    }

    #[test]
    fn retiring_removes_the_plugin_entirely() {
        let a = PluginActivity::new();
        a.publish("run-action", flag(true));
        a.retire("run-action");
        assert!(!a.is_enabled("run-action"));
    }

    #[test]
    fn republishing_lands_on_the_new_vm() {
        // Reload drops a plugin's VM and builds another. If the map kept the dead flag, a
        // sibling would read the old incarnation's state forever.
        let a = PluginActivity::new();
        let old = flag(true);
        a.publish("bevy-runtime", Arc::clone(&old));

        let new = flag(false);
        a.publish("bevy-runtime", Arc::clone(&new));
        assert!(!a.is_enabled("bevy-runtime"));

        new.store(true, Ordering::Relaxed);
        assert!(a.is_enabled("bevy-runtime"));
        // The dead flag has no say any more.
        old.store(false, Ordering::Relaxed);
        assert!(a.is_enabled("bevy-runtime"));
    }

    #[test]
    fn clear_forgets_everything() {
        let a = PluginActivity::new();
        a.publish("one", flag(true));
        a.publish("two", flag(true));
        a.clear();
        assert!(!a.is_enabled("one"));
        assert!(!a.is_enabled("two"));
    }

    /// A `PluginActivity` with those plugins loaded and enabled.
    fn live(names: &[&str]) -> PluginActivity {
        let a = PluginActivity::new();
        for n in names { a.publish(n, flag(true)); }
        a
    }

    #[test]
    fn services_are_listed_qualified_and_ordered() {
        let idx = ServiceIndex::new();
        idx.export("run-monitor", "watch");
        idx.export("cloud-storage", "upload");
        idx.export("run-monitor", "attach");

        // Alphabetical by plugin then method — a discovery call whose order reshuffles
        // between invocations is worse than one that is merely sorted.
        assert_eq!(
            idx.qualified(&live(&["run-monitor", "cloud-storage"])),
            vec!["cloud-storage.upload", "run-monitor.attach", "run-monitor.watch"],
        );
    }

    #[test]
    fn a_disabled_plugin_advertises_nothing() {
        // `arbor.service.call` refuses a disabled plugin, so listing its exports would
        // advertise something that cannot be called.
        let idx = ServiceIndex::new();
        idx.export("run-monitor", "watch");

        let activity = PluginActivity::new();
        let f = flag(true);
        activity.publish("run-monitor", Arc::clone(&f));
        assert_eq!(idx.qualified(&activity), vec!["run-monitor.watch"]);

        f.store(false, Ordering::Relaxed);
        assert!(idx.qualified(&activity).is_empty());
    }

    #[test]
    fn a_plugin_the_host_never_loaded_advertises_nothing() {
        // Nothing publishes a dormant plugin, so an index entry with no activity entry is
        // filtered out rather than trusted.
        let idx = ServiceIndex::new();
        idx.export("ghost", "method");
        assert!(idx.qualified(&PluginActivity::new()).is_empty());
    }

    #[test]
    fn unexport_withdraws_one_method_and_keeps_the_plugin() {
        let idx = ServiceIndex::new();
        idx.export("run-action", "start");
        idx.export("run-action", "stop");
        idx.unexport("run-action", "stop");
        assert_eq!(idx.qualified(&live(&["run-action"])), vec!["run-action.start"]);

        // Still loaded, so it can export again.
        idx.export("run-action", "restart");
        assert_eq!(
            idx.qualified(&live(&["run-action"])),
            vec!["run-action.restart", "run-action.start"],
        );
    }

    #[test]
    fn retiring_a_plugin_drops_all_its_services() {
        let idx = ServiceIndex::new();
        idx.export("source-export", "run");
        idx.retire("source-export");
        assert!(idx.qualified(&live(&["source-export"])).is_empty());
    }

    #[test]
    fn the_index_is_shared_by_clones() {
        // The exporting plugin's VM holds one clone, the listing plugin's another.
        let host_side = ServiceIndex::new();
        let vm_side   = host_side.clone();
        vm_side.export("encoding-guardian", "scan");
        assert_eq!(
            host_side.qualified(&live(&["encoding-guardian"])),
            vec!["encoding-guardian.scan"],
        );
    }

    #[test]
    fn a_clone_shares_the_map() {
        // Every sandbox gets a clone; they must all see the host's publishes.
        let host_side = PluginActivity::new();
        let vm_side   = host_side.clone();
        host_side.publish("shader-preview-meshes", flag(true));
        assert!(vm_side.is_enabled("shader-preview-meshes"));
    }
}
