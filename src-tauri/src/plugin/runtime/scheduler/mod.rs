//! Scheduler engine — fixed_rate / fixed_delay / cron.
//!
//! `PluginHost::start_all_schedulers` spins up one OS thread per registered
//! schedule. Each thread sleeps in 1-second slices so cancellation (disable /
//! reload) takes effect within ~1 s without an extra signalling primitive.

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Manager;

use crate::error::{AppError, Result};

use super::host::PluginHost;
use super::manifest::schedule::{PluginSchedule, ScheduleTrigger};

impl PluginHost {
    pub fn start_all_schedulers(&mut self) {
        // Snapshot every enabled plugin's registered schedules. Plugins whose
        // `[scheduler] enabled = false` (or omitted) are skipped — even if
        // their main.lua called `arbor.scheduler.register`, those entries
        // were rejected at registration time so the list is empty anyway.
        let to_start: Vec<(String, Vec<PluginSchedule>)> = self
            .plugins
            .iter()
            .filter(|p| p.is_enabled() && p.manifest.scheduler.enabled)
            .map(|p| {
                let list = p.schedules.lock().map(|g| g.clone()).unwrap_or_default();
                (p.manifest.name.clone(), list)
            })
            .collect();

        for (name, schedules) in to_start {
            for schedule in schedules {
                if schedule.on_load {
                    let _ = self.fire_hook_on(&name, &schedule.action, "{}");
                }
                self.spawn_scheduler(&name, &schedule);
            }
        }
    }

    pub fn start_plugin_scheduler(&mut self, name: &str, action: &str) -> Result<()> {
        let plugin = self.plugins.iter()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| AppError::Other(format!("plugin '{name}' not found")))?;

        if !plugin.is_enabled() {
            return Err(AppError::Other(format!(
                "plugin '{name}' is disabled — enable it first"
            )));
        }
        if !plugin.manifest.scheduler.enabled {
            return Err(AppError::Other(format!(
                "plugin '{name}' has no [scheduler] section enabled in plugin.toml"
            )));
        }

        let schedule = plugin.schedules.lock()
            .map_err(|_| AppError::Other("schedule registry mutex poisoned".into()))?
            .iter()
            .find(|s| s.action == action)
            .cloned()
            .ok_or_else(|| AppError::Other(format!(
                "no schedule with action '{action}' in plugin '{name}'"
            )))?;

        self.spawn_scheduler(name, &schedule);
        Ok(())
    }

    pub fn stop_plugin_scheduler(&mut self, name: &str, action: &str) -> Result<()> {
        let key = format!("{name}:{action}");
        if let Some(cancel) = self.scheduler_cancels.remove(&key) {
            cancel.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Spawn (or restart) the OS thread driving a single registered schedule.
    /// Re-registration with the same `(plugin_name, action)` key cancels the
    /// previous thread first.
    pub(crate) fn spawn_scheduler(&mut self, plugin_name: &str, schedule: &PluginSchedule) {
        let key = format!("{plugin_name}:{}", schedule.action);

        // Cancel any existing scheduler with the same key — re-registration
        // (or a UI-driven stop+start) replaces the running thread.
        if let Some(old) = self.scheduler_cancels.get(&key) {
            old.store(true, Ordering::Relaxed);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.scheduler_cancels.insert(key, cancel.clone());

        let Some(handle) = self.app_handle.clone() else { return };

        let plugin_name_owned = plugin_name.to_string();
        let action            = schedule.action.clone();
        let trigger           = schedule.trigger.clone();
        let initial_delay_sec = schedule.initial_delay_sec;
        let only_when_focused = schedule.only_when_focused;

        std::thread::Builder::new()
            .name(format!("arbor-sched-{plugin_name_owned}-{action}"))
            .spawn(move || {
                run_scheduler_loop(
                    handle,
                    plugin_name_owned,
                    action,
                    trigger,
                    initial_delay_sec,
                    only_when_focused,
                    cancel,
                );
            })
            .ok();
    }
}

/// Background loop driving a single registered schedule. One thread per
/// schedule. `cancel` is checked between every sleep tick so disable / reload
/// stops the loop within ~1 s.
fn run_scheduler_loop(
    handle:            tauri::AppHandle,
    plugin_name:       String,
    action:            String,
    trigger:           ScheduleTrigger,
    initial_delay_sec: u64,
    only_when_focused: bool,
    cancel:            Arc<AtomicBool>,
) {
    // Initial delay — applies to fixed_rate / fixed_delay (cron is always
    // anchored to the wall clock, never to "now + N").
    if initial_delay_sec > 0 && !matches!(trigger, ScheduleTrigger::Cron { .. }) {
        if !sleep_with_cancel(initial_delay_sec, &cancel) { return; }
    }

    // Cron schedules need parsing once up front. A malformed expression is
    // logged and the loop exits — no point spinning forever.
    let cron_schedule = match &trigger {
        ScheduleTrigger::Cron { expr } => match cron::Schedule::from_str(expr) {
            Ok(s)  => Some(s),
            Err(e) => {
                tracing::error!(
                    "plugin '{plugin_name}': invalid cron expression '{expr}': {e} \
                     — scheduler not started"
                );
                return;
            }
        },
        _ => None,
    };

    // Tracks the start time of the previous fire so FixedRate can compute
    // "interval since previous start" rather than "interval since previous
    // end" (= FixedDelay). On overrun the next fire happens immediately.
    let mut last_fire_start: Option<std::time::Instant> = None;

    loop {
        if cancel.load(Ordering::Relaxed) { break; }

        // Compute how long to sleep before the next fire.
        let wait = match (&trigger, &cron_schedule) {
            (ScheduleTrigger::FixedRate { interval_sec }, _) => {
                match last_fire_start {
                    None    => *interval_sec,
                    Some(t) => interval_sec.saturating_sub(t.elapsed().as_secs()),
                }
            }
            (ScheduleTrigger::FixedDelay { delay_sec }, _) => *delay_sec,
            (ScheduleTrigger::Cron { .. }, Some(sched)) => {
                let now = chrono::Utc::now();
                match sched.upcoming(chrono::Utc).next() {
                    Some(next) => {
                        let delta = (next - now).num_seconds();
                        if delta < 0 { 0 } else { delta as u64 }
                    }
                    None => break, // no future occurrence — done
                }
            }
            _ => break,
        };

        if wait > 0 && !sleep_with_cancel(wait, &cancel) { break; }

        // Skip when focus-gated and the window is in the background. The
        // clock keeps advancing — we just don't fire this tick.
        let state = handle.state::<crate::AppState>();
        if only_when_focused && !state.app_focused.load(Ordering::Relaxed) {
            // Mark the (skipped) fire time so FixedRate doesn't catch up
            // with a burst of back-to-back fires when focus returns.
            last_fire_start = Some(std::time::Instant::now());
            continue;
        }

        last_fire_start = Some(std::time::Instant::now());
        // Fire. For fixed_delay the handler runs synchronously inside the
        // host's plugin lock — the next sleep starts after `fire_hook_on`
        // returns, which gives us the desired "wait N after handler" cadence.
        if let Ok(host) = state.plugin_host.lock() {
            let _ = host.fire_hook_on(&plugin_name, &action, "{}");
        };
    }
}

/// Sleep `secs` seconds in 1-second slices, returning false if `cancel`
/// flipped before the wait completed.
fn sleep_with_cancel(secs: u64, cancel: &Arc<AtomicBool>) -> bool {
    let mut left = secs;
    while left > 0 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if cancel.load(Ordering::Relaxed) { return false; }
        left -= 1;
    }
    true
}
