//! Marketplace auto-refresh interval **setters** — the keep-shell remainder.
//!
//! Catalog fetches, plugin/theme install/uninstall, the enable toggle, and the
//! custom-source resolver all migrated to the generic router
//! ([`crate::ipc::platform::marketplace`]). The two interval setters stay here:
//! they take an `AppHandle` to re-arm the **running** refresh scheduler on the
//! fly, which the router's type-erased `&AppState` context can't carry.

use tauri::State;

use crate::error::{AppError, Result};
use crate::marketplace;
use crate::AppState;

/// Set the auto-refresh interval in hours. `None` or `Some(0)` disables the
/// scheduler. The change takes effect on the next poll cycle.
#[tauri::command]
pub fn marketplace_set_refresh_hours(
    app:   tauri::AppHandle,
    state: State<'_, AppState>,
    hours: Option<u32>,
) -> Result<()> {
    let normalized = match hours {
        Some(0) => None,
        other   => other,
    };
    let snapshot = {
        let mut cfg = state.lock_config()?;
        cfg.marketplace.refresh_hours = normalized;
        cfg.clone()
    };
    crate::config::app_config::save(&snapshot)
        .map_err(|e| AppError::Other(format!("could not persist marketplace refresh hours: {e}")))?;
    // Park / re-arm the running schedule without restarting it.
    marketplace::scheduler::apply_refresh_hours(&app, normalized);
    Ok(())
}

/// How often the background scheduler wakes up to check whether a refresh is
/// due. Clamped to [1, 60] in the scheduler regardless of what's persisted.
#[tauri::command]
pub fn marketplace_set_poll_minutes(
    app:     tauri::AppHandle,
    state:   State<'_, AppState>,
    minutes: u32,
) -> Result<()> {
    let clamped = minutes.clamp(1, 60);
    let snapshot = {
        let mut cfg = state.lock_config()?;
        cfg.marketplace.poll_minutes = clamped;
        cfg.clone()
    };
    crate::config::app_config::save(&snapshot)
        .map_err(|e| AppError::Other(format!("could not persist marketplace poll minutes: {e}")))?;
    // Swap the running schedule's cadence on the fly.
    marketplace::scheduler::apply_poll_minutes(&app, clamped);
    Ok(())
}
