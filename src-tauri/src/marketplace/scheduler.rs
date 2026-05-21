//! Background auto-refresh scheduler for the marketplace catalog.
//!
//! Arbor is meant to stay open for long stretches — a fetch only on modal
//! open isn't enough. This task wakes every `poll_minutes` minutes, reads
//! the user's configured interval, and re-fetches if the cache has aged
//! past it. Failures are logged but never surfaced — the next manual
//! Refresh is always available.
//!
//! Config lives in `AppConfig.marketplace`:
//!   * `refresh_hours`: `Some(0)` / `None` → auto-refresh disabled,
//!                      `Some(n)`           → refresh every n hours.
//!   * `poll_minutes`:  how often the scheduler wakes up to decide
//!                      whether to fire (default 10, clamped to [1, 60]).
//!
//! Reads the config on every poll, so changes propagate within at most
//! one poll cycle without needing to restart the scheduler.

use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use crate::AppState;

use super::cache;

/// Lower bound for the poll cadence — anything finer is wasted work.
const MIN_POLL_MINUTES: u32 = 1;
/// Upper bound — going past an hour starts to lag too far behind setting
/// changes (toggle off → still fires once for up to an hour).
const MAX_POLL_MINUTES: u32 = 60;
/// Fallback used when the config value is missing or out of range.
const DEFAULT_POLL_MINUTES: u32 = 10;

/// Hand-off from the Tauri `setup()` callback. Spawns a long-lived
/// background task; never blocks the caller.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        run(app).await;
    });
}

async fn run(app: AppHandle) {
    loop {
        let poll_secs = read_poll_secs(&app);
        tokio::time::sleep(std::time::Duration::from_secs(poll_secs)).await;

        let interval_secs = match read_interval_secs(&app) {
            Some(s) => s,
            None    => continue, // user disabled auto-refresh
        };

        let age = current_cache_age_secs();
        if age < interval_secs {
            continue;
        }

        // Cache is stale (or missing). Fire the refresh.
        let state = app.state::<AppState>();
        match super::refresh_community(&state.marketplace).await {
            Ok(()) => tracing::info!(
                "marketplace auto-refresh: catalog refreshed (was {age}s old, interval {interval_secs}s)"
            ),
            Err(e) => tracing::warn!("marketplace auto-refresh failed: {e}"),
        }
    }
}

/// Resolve the configured refresh interval in seconds, or `None` when the
/// user has disabled the scheduler (refresh_hours = 0 or unset).
fn read_interval_secs(app: &AppHandle) -> Option<u64> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().ok()?;
    let hours = cfg.marketplace.refresh_hours?;
    if hours == 0 { return None; }
    Some(u64::from(hours) * 3600)
}

/// Resolve the configured polling cadence in seconds, clamped to a sane
/// range. Falls back to the default when the lock fails or the value
/// is out of bounds.
fn read_poll_secs(app: &AppHandle) -> u64 {
    let state = app.state::<AppState>();
    let raw = state.config.lock().ok()
        .map(|c| c.marketplace.poll_minutes)
        .unwrap_or(DEFAULT_POLL_MINUTES);
    let clamped = raw.clamp(MIN_POLL_MINUTES, MAX_POLL_MINUTES);
    u64::from(clamped) * 60
}

/// Seconds since the on-disk cache was last written. `u64::MAX` when the
/// cache is missing — that's "infinitely stale" so the next poll fires
/// immediately.
fn current_cache_age_secs() -> u64 {
    let Some(file) = cache::load_any() else { return u64::MAX; };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    now.saturating_sub(file.fetched_at)
}
