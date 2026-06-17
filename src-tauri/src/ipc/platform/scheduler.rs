//! `scheduler` domain — read-only surface over the shared `arbor-scheduler`
//! engine.
//!
//! Powers the Command Palette → "Show Active Schedules" modal. `list_schedules`
//! returns a snapshot of every currently registered schedule (plugin actions,
//! marketplace auto-refresh, future pipeline timers, …). It's a pure read with
//! no side effects, so it migrates cleanly.
//!
//! Any (re)arm / install handler that schedules an OS timer or holds
//! marketplace-setter side effects stays inline in the old command module — but
//! the only command in that module is this read, so nothing is deferred here.
//!
//! No hooks fire in this domain.

use arbor_scheduler::prelude::ScheduleSnapshot;

use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

/// Snapshot every schedule currently registered against the shared engine.
/// Returns an empty vector during the boot window before the scheduler is
/// installed, rather than failing — the modal renders an empty state in
/// that case instead of an error toast.
#[platform::handler(program = "platform")]
fn list_schedules(state: &AppState) -> Result<Vec<ScheduleSnapshot>, AppError> {
    Ok(state.scheduler().map(|s| s.list()).unwrap_or_default())
}
