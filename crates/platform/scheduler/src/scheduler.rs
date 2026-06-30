//! Public engine surface: register / cancel / list / set_enabled /
//! update_trigger.
//!
//! Each registered entry owns a tokio task driven by [`crate::runner::run`];
//! the `Scheduler` itself just routes mutations into the per-entry shared
//! state and notifies the corresponding runner so it re-evaluates on the
//! next loop turn (instead of waiting for the current sleep to elapse).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arbor_core::prelude::AppCtx;
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::action::ArcAction;
use crate::error::{Result, SchedulerError};
use crate::key::ScheduleKey;
use crate::opts::ScheduleOpts;
use crate::runner;
use crate::snapshot::ScheduleSnapshot;
use crate::trigger::{CompiledTrigger, Trigger};

/// Bundle of trigger state held under a single short-lived `Mutex`.
///
/// Both forms live together so the runner takes one lock per sleep
/// computation: the parsed form to compute the next wait, the public form
/// for [`Scheduler::list`] snapshots. The lock is never held across an
/// `.await`, so `std::sync::Mutex` is the right pick (and mutators don't
/// block firing in progress).
pub(crate) struct TriggerState {
    pub(crate) public:   Trigger,
    pub(crate) compiled: CompiledTrigger,
}

/// Shared per-entry state. Cloned (as `Arc`) between the runner task and
/// the `Scheduler` map; mutated through atomics + a single mutex.
pub(crate) struct EntryShared {
    pub(crate) key:       ScheduleKey,
    pub(crate) opts:      ScheduleOpts,
    pub(crate) action:    ArcAction,
    pub(crate) trigger:   std::sync::Mutex<TriggerState>,
    pub(crate) enabled:   AtomicBool,
    pub(crate) cancelled: AtomicBool,
    /// `notify_one`d on every state change so a sleeping runner wakes up
    /// and re-reads its state immediately. Permit-storing semantics
    /// matter — a mutation that fires before the runner registers as a
    /// waiter must not be lost.
    pub(crate) wake:      Notify,
}

struct Entry {
    shared: Arc<EntryShared>,
    handle: JoinHandle<()>,
}

pub struct Scheduler {
    ctx:     Arc<dyn AppCtx>,
    /// Tokio runtime the per-schedule tasks are spawned on. Passed in
    /// explicitly so callers can `register` from any context (sync
    /// `setup()` callbacks, plugin lifecycle code holding a mutex, …)
    /// without needing a "current runtime" already in scope.
    rt:      Handle,
    entries: std::sync::Mutex<HashMap<ScheduleKey, Entry>>,
}

impl Scheduler {
    pub fn new(ctx: Arc<dyn AppCtx>, rt: Handle) -> Self {
        Self {
            ctx,
            rt,
            entries: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Register a schedule. Replaces any existing entry with the same key
    /// — the previous runner is asked to exit and aborted as a backstop.
    /// Fails only if `trigger` is a malformed cron expression.
    pub fn register(
        &self,
        key:     ScheduleKey,
        trigger: Trigger,
        opts:    ScheduleOpts,
        action:  ArcAction,
    ) -> Result<()> {
        self.register_with(key, trigger, opts, action, true)
    }

    /// Like [`Self::register`] but lets the caller pick the initial
    /// enabled state. A schedule registered as `enabled = false` parks
    /// immediately; [`Self::set_enabled`] flips it on with no work for
    /// the consumer.
    pub fn register_with(
        &self,
        key:     ScheduleKey,
        trigger: Trigger,
        opts:    ScheduleOpts,
        action:  ArcAction,
        enabled: bool,
    ) -> Result<()> {
        let compiled = CompiledTrigger::try_compile(&trigger)?;

        let shared = Arc::new(EntryShared {
            key:       key.clone(),
            opts,
            action,
            trigger:   std::sync::Mutex::new(TriggerState { public: trigger, compiled }),
            enabled:   AtomicBool::new(enabled),
            cancelled: AtomicBool::new(false),
            wake:      Notify::new(),
        });

        let handle = self.rt.spawn(runner::run(shared.clone(), self.ctx.clone()));

        let mut entries = self.entries.lock().expect("scheduler entries poisoned");
        if let Some(old) = entries.insert(key, Entry { shared, handle }) {
            stop_entry(&old);
        }
        Ok(())
    }

    /// Cancel a single schedule. Returns whether anything was cancelled.
    pub fn cancel(&self, key: &ScheduleKey) -> bool {
        let removed = {
            let mut entries = self.entries.lock().expect("scheduler entries poisoned");
            entries.remove(key)
        };
        match removed {
            Some(entry) => { stop_entry(&entry); true }
            None        => false,
        }
    }

    /// Cancel every schedule whose namespace equals `namespace` exactly
    /// (no prefix match — that would catch unrelated plugins whose names
    /// happen to share a prefix, e.g. `foo` and `foobar`). Useful for
    /// "unload plugin X" or "shut down the marketplace subsystem".
    /// Returns the number of schedules cancelled.
    pub fn cancel_namespace(&self, namespace: &str) -> usize {
        let removed: Vec<Entry> = {
            let mut entries = self.entries.lock().expect("scheduler entries poisoned");
            let keys: Vec<ScheduleKey> = entries.keys()
                .filter(|k| k.namespace == namespace)
                .cloned()
                .collect();
            keys.into_iter().filter_map(|k| entries.remove(&k)).collect()
        };
        for entry in &removed {
            stop_entry(entry);
        }
        removed.len()
    }

    /// Toggle a schedule on or off without dropping its task. A disabled
    /// schedule parks on [`EntryShared::wake`] until re-enabled — no
    /// thread teardown, no reconfiguration on the consumer side. Returns
    /// whether the key existed.
    pub fn set_enabled(&self, key: &ScheduleKey, enabled: bool) -> bool {
        let entries = self.entries.lock().expect("scheduler entries poisoned");
        match entries.get(key) {
            Some(entry) => {
                entry.shared.enabled.store(enabled, Ordering::Release);
                entry.shared.wake.notify_one();
                true
            }
            None => false,
        }
    }

    /// Swap the trigger of a running schedule. The runner picks up the new
    /// cadence on its next wake-up, which is immediate — the swap notifies.
    /// Errors on `NotFound` or on a malformed cron expression; on error,
    /// the previous trigger stays in effect.
    pub fn update_trigger(&self, key: &ScheduleKey, trigger: Trigger) -> Result<()> {
        let compiled = CompiledTrigger::try_compile(&trigger)?;

        let shared = {
            let entries = self.entries.lock().expect("scheduler entries poisoned");
            entries.get(key).map(|e| e.shared.clone())
        };
        let shared = shared.ok_or_else(|| SchedulerError::NotFound(key.to_string()))?;

        {
            let mut guard = shared.trigger.lock().expect("trigger state poisoned");
            guard.public   = trigger;
            guard.compiled = compiled;
        }
        shared.wake.notify_one();
        Ok(())
    }

    /// Snapshot every currently registered schedule. Order is unspecified.
    pub fn list(&self) -> Vec<ScheduleSnapshot> {
        let entries = self.entries.lock().expect("scheduler entries poisoned");
        entries.values()
            .map(|e| {
                let trigger = e.shared.trigger.lock()
                    .expect("trigger state poisoned").public.clone();
                ScheduleSnapshot {
                    key:               e.shared.key.clone(),
                    trigger,
                    enabled:           e.shared.enabled.load(Ordering::Acquire),
                    fire_on_load:      e.shared.opts.fire_on_load,
                    only_when_focused: e.shared.opts.only_when_focused,
                }
            })
            .collect()
    }

    pub fn contains(&self, key: &ScheduleKey) -> bool {
        self.entries.lock().expect("scheduler entries poisoned").contains_key(key)
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        let mut entries = self.entries.lock().expect("scheduler entries poisoned");
        for (_, entry) in entries.drain() {
            stop_entry(&entry);
        }
    }
}

/// Common teardown: flip the cancel flag, wake the runner so it observes
/// the flag immediately, and abort the JoinHandle as a backstop in case
/// the action is in the middle of a long `.await`.
fn stop_entry(entry: &Entry) {
    entry.shared.cancelled.store(true, Ordering::Release);
    entry.shared.wake.notify_one();
    entry.handle.abort();
}
