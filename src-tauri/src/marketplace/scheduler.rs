//! Marketplace auto-refresh — one entry in the shared `arbor-scheduler`
//! engine.
//!
//! Behaviour mirrored from the previous bespoke loop:
//!
//!   * `refresh_hours = None` / `Some(0)` → auto-refresh disabled.
//!   * `refresh_hours = Some(n)`          → refresh whenever the on-disk
//!                                          cache is older than `n` hours.
//!   * `poll_minutes`  → how often the engine wakes up to re-evaluate
//!                       the "is it time?" gate (clamped to [1, 60]).
//!
//! The two settings are exposed via [`apply_refresh_hours`] /
//! [`apply_poll_minutes`], called from `marketplace_set_*` commands so
//! the running schedule reconfigures on the fly — no app restart.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arbor_scheduler::prelude::*;
use tauri::{AppHandle, Manager};

use crate::AppState;
use super::cache;

const NAMESPACE: &str = "marketplace";
const NAME:      &str = "auto_refresh";

const MIN_POLL_MINUTES:     u32 = 1;
const MAX_POLL_MINUTES:     u32 = 60;
const DEFAULT_POLL_MINUTES: u32 = 10;

fn key() -> ScheduleKey {
    ScheduleKey::new(NAMESPACE, NAME)
}

/// Register the auto-refresh schedule against the shared engine. Called
/// from Tauri `setup()` after the scheduler is installed in `AppState`.
/// Failure is logged + swallowed — a broken auto-refresh must never
/// prevent the app from booting; manual `Refresh` stays available.
pub fn install(app: AppHandle) {
    let state = app.state::<AppState>();
    let Some(sched) = state.scheduler() else {
        tracing::warn!("marketplace scheduler: shared engine not available — skipping");
        return;
    };

    let (poll_minutes, hours) = read_settings(&app);
    let trigger = trigger_for(poll_minutes);
    let enabled = hours.is_some();

    let app_for_action = app.clone();
    let app_for_gate   = app.clone();

    let opts = ScheduleOpts {
        gate: Some(Arc::new(move || is_refresh_due(&app_for_gate))),
        ..Default::default()
    };

    let action: ArcAction = Arc::new(FnAction(move || {
        let app = app_for_action.clone();
        async move {
            let state = app.state::<AppState>();
            match super::refresh_community(&state.marketplace).await {
                Ok(()) => tracing::info!("marketplace auto-refresh: catalog refreshed"),
                Err(e) => tracing::warn!("marketplace auto-refresh failed: {e}"),
            }
        }
    }));

    if let Err(e) = sched.register_with(key(), trigger, opts, action, enabled) {
        tracing::warn!("marketplace scheduler: register failed: {e}");
    }
}

/// On-the-fly reconfiguration when the user toggles `refresh_hours`.
/// `None` / `Some(0)` parks the schedule (no thread teardown — the entry
/// stays registered, just disabled). Any positive value re-enables it.
pub fn apply_refresh_hours(app: &AppHandle, hours: Option<u32>) {
    let Some(sched) = app.state::<AppState>().scheduler() else { return; };
    let enabled = matches!(hours, Some(n) if n > 0);
    sched.set_enabled(&key(), enabled);
}

/// On-the-fly reconfiguration when the user changes `poll_minutes`.
/// Swaps the trigger; the running task picks up the new cadence on its
/// very next wake-up.
pub fn apply_poll_minutes(app: &AppHandle, minutes: u32) {
    let Some(sched) = app.state::<AppState>().scheduler() else { return; };
    if let Err(e) = sched.update_trigger(&key(), trigger_for(minutes)) {
        tracing::warn!("marketplace scheduler: update_trigger failed: {e}");
    }
}

fn trigger_for(minutes: u32) -> Trigger {
    let clamped = minutes.clamp(MIN_POLL_MINUTES, MAX_POLL_MINUTES);
    Trigger::FixedDelay { delay: Duration::from_secs(u64::from(clamped) * 60) }
}

fn read_settings(app: &AppHandle) -> (u32, Option<u32>) {
    let state = app.state::<AppState>();
    let Ok(cfg) = state.config.lock() else {
        return (DEFAULT_POLL_MINUTES, None);
    };
    (cfg.marketplace.poll_minutes, cfg.marketplace.refresh_hours)
}

/// Gate predicate evaluated by the engine on every tick. `false` skips
/// the fire without unscheduling the task — settings can flip back on
/// without re-registering.
fn is_refresh_due(app: &AppHandle) -> bool {
    let (_, hours) = read_settings(app);
    let Some(h) = hours.filter(|h| *h > 0) else { return false; };
    let interval = u64::from(h) * 3600;
    current_cache_age_secs() >= interval
}

/// Seconds since the cache was last written. `u64::MAX` when the cache
/// is missing — that's "infinitely stale" so the next gate evaluation
/// fires immediately.
fn current_cache_age_secs() -> u64 {
    let Some(file) = cache::load_any() else { return u64::MAX; };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    now.saturating_sub(file.fetched_at)
}
