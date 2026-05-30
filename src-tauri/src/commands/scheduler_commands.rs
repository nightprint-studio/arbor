//! Read-only surface over the shared `arbor-scheduler` engine.
//!
//! Powers the Command Palette → "Show Active Schedules" modal. Returns a
//! snapshot of every currently registered schedule (plugin actions,
//! marketplace auto-refresh, future pipeline timers, …) so the user can
//! see at a glance what's ticking in the background, with what cadence,
//! and whether it's enabled.

use tauri::State;

use arbor_scheduler::prelude::ScheduleSnapshot;

use crate::error::AppError;
use crate::AppState;

/// Snapshot every schedule currently registered against the shared engine.
/// Returns an empty vector during the boot window before the scheduler is
/// installed, rather than failing — the modal renders an empty state in
/// that case instead of an error toast.
#[tauri::command]
pub fn list_schedules(state: State<'_, AppState>) -> Result<Vec<ScheduleSnapshot>, AppError> {
    Ok(state.scheduler().map(|s| s.list()).unwrap_or_default())
}
