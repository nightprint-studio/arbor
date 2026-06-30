//! Per-plugin live state held by the host: an active `LoadedPlugin` (with its
//! own Lua VM) or a `DormantPlugin` (manifest only — no VM until the user
//! enables it).

use std::collections::HashMap;
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
