//! Canonical entry point for `bennu-scheduling`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_scheduling::prelude::...`.

// The extension itself — what a host registers.
pub use crate::ext::{
    SchedulingExtension, CODE_BAD_CRON, CODE_NOT_ENABLED, CODE_NO_TRIGGER,
};

// Cron expressions: valid or not, and in words.
pub use crate::cron::{check as check_cron, describe as describe_cron, CronError, Dialect};

// The jobs themselves.
pub use crate::jobs::{enables_scheduling, jobs_in, Job, Trigger};
