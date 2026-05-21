//! Per-plugin live state held by the host: an active `LoadedPlugin` (with its
//! own Lua VM) or a `DormantPlugin` (manifest only — no VM until the user
//! enables it).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::manifest::PluginManifest;
use super::manifest::schedule::ScheduleRegistry;

// ---------------------------------------------------------------------------
// Timer registry — tracks per-plugin timers spawned from Lua
// ---------------------------------------------------------------------------

pub type TimerCancels = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;
pub type TimerCounter = Arc<AtomicU64>;

/// A plugin discovered on disk but kept in standby because its persisted
/// state is "disabled". No Lua VM, no `main.lua` execution, no schedulers —
/// promoting it to a `LoadedPlugin` happens only when the user explicitly
/// enables it.
pub struct DormantPlugin {
    pub manifest: PluginManifest,
}

pub struct LoadedPlugin {
    pub manifest:      PluginManifest,
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
