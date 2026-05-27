//! Failures originating inside the scheduler crate.
//!
//! Kept intentionally narrow: the only fallible operations the public API
//! exposes are "compile a cron expression" (at [`crate::Scheduler::register`]
//! / [`crate::Scheduler::update_trigger`] time) and "find an existing key".
//! Everything else — sleep, fire, gate — is infallible from the scheduler's
//! point of view.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    /// No schedule is registered with the requested key. Returned by
    /// [`crate::Scheduler::update_trigger`].
    #[error("schedule not found: {0}")]
    NotFound(String),

    /// The cron expression handed to [`crate::Scheduler::register`] or
    /// [`crate::Scheduler::update_trigger`] did not parse. The `message`
    /// is the upstream parser's error, stringified — we don't re-export
    /// `cron`'s error type so the public surface stays stable across
    /// upstream version bumps.
    #[error("invalid cron expression '{expr}': {message}")]
    InvalidCron { expr: String, message: String },
}

pub type Result<T> = std::result::Result<T, SchedulerError>;
