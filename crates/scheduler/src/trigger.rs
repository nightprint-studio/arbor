//! Trigger types + parse-once internal form.
//!
//! Mirrors Spring's `TaskScheduler` semantics:
//!
//! - `FixedRate`  — next fire = previous *start* + `interval`. A handler
//!   that ran longer than the interval triggers the next fire immediately
//!   (no catch-up burst — the missed ticks are collapsed into one).
//! - `FixedDelay` — next fire = previous *end* + `delay`. Handler runtime
//!   counts toward the gap, so back-pressure is implicit.
//! - `Cron`       — anchored to wall-clock matches of a 6-field Spring
//!   cron expression (`sec min hour dom mon dow`).

use std::str::FromStr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::SchedulerError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    FixedRate  { interval: Duration },
    FixedDelay { delay:    Duration },
    Cron       { expr:     String },
}

/// Parsed form, built once at register / update time so the runner loop
/// never re-parses the cron string on every iteration. Not part of the
/// public API — exists only inside [`crate::scheduler::TriggerState`].
pub(crate) enum CompiledTrigger {
    FixedRate(Duration),
    FixedDelay(Duration),
    Cron(Box<cron::Schedule>),
}

impl CompiledTrigger {
    pub(crate) fn try_compile(t: &Trigger) -> Result<Self, SchedulerError> {
        Ok(match t {
            Trigger::FixedRate  { interval } => Self::FixedRate(*interval),
            Trigger::FixedDelay { delay }    => Self::FixedDelay(*delay),
            Trigger::Cron { expr } => {
                let schedule = cron::Schedule::from_str(expr).map_err(|e| {
                    SchedulerError::InvalidCron {
                        expr:    expr.clone(),
                        message: e.to_string(),
                    }
                })?;
                Self::Cron(Box::new(schedule))
            }
        })
    }

    pub(crate) fn is_cron(&self) -> bool {
        matches!(self, Self::Cron(_))
    }

    /// How long the runner should sleep before the next fire, given when
    /// the previous fire *started*. Returns [`NextWait::Done`] only for
    /// the (vanishingly rare) case of a cron schedule whose upcoming
    /// iterator is exhausted.
    pub(crate) fn next_wait(&self, last_start: Option<Instant>) -> NextWait {
        match self {
            Self::FixedRate(interval) => NextWait::Sleep(match last_start {
                None    => *interval,
                Some(t) => interval.saturating_sub(t.elapsed()),
            }),
            Self::FixedDelay(delay) => NextWait::Sleep(*delay),
            Self::Cron(schedule) => {
                let now = chrono::Utc::now();
                match schedule.upcoming(chrono::Utc).next() {
                    None => NextWait::Done,
                    Some(next) => {
                        let ms = (next - now).num_milliseconds();
                        if ms <= 0 {
                            NextWait::Sleep(Duration::ZERO)
                        } else {
                            NextWait::Sleep(Duration::from_millis(ms as u64))
                        }
                    }
                }
            }
        }
    }
}

pub(crate) enum NextWait {
    Sleep(Duration),
    Done,
}
