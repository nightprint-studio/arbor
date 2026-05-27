//! Bridge between `PluginHost` and the shared `arbor-scheduler` engine.
//!
//! The actual loop lives in `arbor-scheduler`; this module owns:
//!
//!   * the [`LuaHookAction`] bridge (Lua hook fired on every tick),
//!   * the (plugin_name, action) → [`ScheduleKey`] mapping,
//!   * the trigger/options translation from the plugin manifest's
//!     [`PluginSchedule`] shape to the engine's [`Trigger`] /
//!     [`ScheduleOpts`].
//!
//! Public API (registered / cancelled per plugin) remains on `PluginHost`:
//! [`start_all_schedulers`], [`start_plugin_scheduler`],
//! [`stop_plugin_scheduler`], [`spawn_scheduler`].

mod lua_action;

use std::sync::Arc;
use std::time::Duration;

use arbor_scheduler::prelude::*;

use crate::error::{AppError, Result};

use self::lua_action::LuaHookAction;
use super::host::PluginHost;
use super::manifest::schedule::{PluginSchedule, ScheduleTrigger};

/// Namespace under which `(plugin_name, action)` schedules live in the
/// shared engine. Built per-plugin so `cancel_namespace` cleanly maps to
/// "cancel every schedule owned by this plugin" without prefix-collision
/// risk (the engine uses exact equality, not `starts_with`).
pub(crate) fn plugin_namespace(plugin_name: &str) -> String {
    format!("plugin:{plugin_name}")
}

fn schedule_key(plugin_name: &str, action: &str) -> ScheduleKey {
    ScheduleKey::new(plugin_namespace(plugin_name), action)
}

fn trigger_from(t: &ScheduleTrigger) -> Trigger {
    match t {
        ScheduleTrigger::FixedRate  { interval_sec } =>
            Trigger::FixedRate  { interval: Duration::from_secs(*interval_sec) },
        ScheduleTrigger::FixedDelay { delay_sec } =>
            Trigger::FixedDelay { delay:    Duration::from_secs(*delay_sec) },
        ScheduleTrigger::Cron { expr } =>
            Trigger::Cron       { expr:     expr.clone() },
    }
}

fn opts_from(s: &PluginSchedule) -> ScheduleOpts {
    ScheduleOpts {
        initial_delay:     Duration::from_secs(s.initial_delay_sec),
        fire_on_load:      s.on_load,
        only_when_focused: s.only_when_focused,
        gate:              None,
    }
}

impl PluginHost {
    /// Register every schedule declared by every loaded + enabled plugin
    /// against the shared engine. No-op when the engine isn't installed
    /// yet (boot-phase ordering) or when no plugins are loaded.
    pub fn start_all_schedulers(&mut self) {
        // Snapshot every enabled plugin's registered schedules. Plugins
        // whose `[scheduler] enabled = false` (or omitted) are skipped —
        // even if their main.lua called `arbor.scheduler.register`, those
        // entries were rejected at registration time so the list is empty.
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
        if let Some(sched) = &self.scheduler {
            sched.cancel(&schedule_key(name, action));
        }
        Ok(())
    }

    /// Register (or replace) a single schedule against the shared engine.
    /// Re-registration with the same `(plugin_name, action)` key cancels
    /// the previous task automatically. No-op when the engine hasn't been
    /// installed yet (boot-phase ordering) or when the host has no
    /// upgradable self-reference (shouldn't happen after `setup()`).
    pub(crate) fn spawn_scheduler(&mut self, plugin_name: &str, schedule: &PluginSchedule) {
        let Some(sched) = self.scheduler.clone() else {
            tracing::debug!(
                "spawn_scheduler('{plugin_name}:{}') called before scheduler install — skipping",
                schedule.action,
            );
            return;
        };
        let Some(self_arc) = self.self_arc.clone() else {
            tracing::warn!(
                "spawn_scheduler('{plugin_name}:{}'): host self-pointer missing — skipping",
                schedule.action,
            );
            return;
        };

        let action: ArcAction = Arc::new(LuaHookAction {
            host:        self_arc,
            plugin_name: plugin_name.to_string(),
            action_name: schedule.action.clone(),
        });

        if let Err(e) = sched.register(
            schedule_key(plugin_name, &schedule.action),
            trigger_from(&schedule.trigger),
            opts_from(schedule),
            action,
        ) {
            tracing::warn!(
                "scheduler register failed for '{plugin_name}:{}': {e}",
                schedule.action,
            );
        }
    }
}
