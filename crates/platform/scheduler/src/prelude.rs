//! Canonical entry point for `arbor-scheduler`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers reach types through
//! `arbor_scheduler::prelude::...` (or `use arbor_scheduler::prelude::*;`
//! once per file) rather than the per-feature submodule paths. The
//! submodules stay `pub` for rustdoc navigation only.

pub use crate::action::{Action, ArcAction, FnAction};
pub use crate::error::{Result, SchedulerError};
pub use crate::key::ScheduleKey;
pub use crate::opts::{Gate, ScheduleOpts};
pub use crate::scheduler::Scheduler;
pub use crate::snapshot::ScheduleSnapshot;
pub use crate::trigger::{Trigger, validate_cron};
pub use crate::calendar::{due, format_local, parse_clock, parse_local_datetime, parse_weekday, next_after, next_after_in, validate as validate_recurrence, Due, MissedPolicy, Recurrence, GRACE_MS, MIN_INTERVAL_SECS};
pub use crate::calendar::{period_starts, period_starts_in};
pub use crate::jobs::{clip, new_id, now_ms, when_to_recurrence, Job, JobAction, JobInput, LastRun, Reader, Store, Tick, MAX_DETAIL_CHARS, MAX_NAME_CHARS, MAX_PROMPT_CHARS};
