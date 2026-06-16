//! Canonical entry point for `arbor-feedback`'s public API.
//!
//! Consumers reach types and functions through `arbor_feedback::prelude::…`
//! (or a single `use arbor_feedback::prelude::*;`) rather than the per-feature
//! submodules. The submodules stay `pub` for rustdoc navigation.

pub use crate::jobs::{kill_process, JobInfo, JobRegistry, JobStatus};
pub use crate::notify::{emit_notification, NotificationPayload, EVENT_NOTIFICATION};
pub use crate::operations::{EVENT_OP_FINISH, EVENT_OP_START, EVENT_OP_UPDATE};
