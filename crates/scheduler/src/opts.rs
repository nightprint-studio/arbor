//! Optional per-schedule knobs.

use std::sync::Arc;
use std::time::Duration;

/// Synchronous per-tick filter. Returning `false` skips the fire without
/// killing the runner — the clock keeps advancing and the next tick is
/// re-evaluated normally. Lets consumers express "runtime feature flag"
/// semantics like the marketplace `refresh_hours = 0` toggle without
/// having to cancel + re-register the schedule on every toggle.
///
/// Kept synchronous on purpose: every real-world gate (config read,
/// atomic, hashmap lookup) is microseconds. An `async` variant can be
/// added later if a network-bound gate ever shows up.
pub type Gate = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Default, Clone)]
pub struct ScheduleOpts {
    /// Wait before the first fire. Applies to `FixedRate` / `FixedDelay`
    /// only — cron schedules are always anchored to the next wall-clock
    /// occurrence, so "wait N from now" would just delay the first match
    /// pointlessly.
    pub initial_delay: Duration,

    /// Fire once at registration, before the cadence starts. Useful for
    /// "warm caches now, then keep them warm" loops. Ignored when the
    /// schedule is registered already disabled.
    pub fire_on_load: bool,

    /// Skip firing while the host window is not focused. The clock keeps
    /// advancing — `FixedRate` doesn't catch up with a burst when focus
    /// returns.
    pub only_when_focused: bool,

    /// Optional per-tick predicate; see [`Gate`].
    pub gate: Option<Gate>,
}
