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
