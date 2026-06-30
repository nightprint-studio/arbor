//! Frontend-friendly snapshot of a registered schedule.

use serde::Serialize;

use crate::key::ScheduleKey;
use crate::trigger::Trigger;

/// Returned by [`crate::Scheduler::list`]. `Serialize` so consumers can
/// pass it straight through IPC if they want a "what's currently running"
/// surface in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ScheduleSnapshot {
    pub key:               ScheduleKey,
    pub trigger:           Trigger,
    pub enabled:           bool,
    pub fire_on_load:      bool,
    pub only_when_focused: bool,
}
