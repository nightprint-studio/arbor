//! Identifier for a registered schedule.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Two-part key so consumers can scope bulk operations by namespace without
/// resorting to substring matching on a flat string.
///
///   * `namespace` — the consumer subsystem (`"plugin"`, `"marketplace"`,
///     `"pipeline"`, …).
///   * `name`      — consumer-local identifier. For the plugin runtime this
///     is conventionally `"<plugin>:<action>"`; for the marketplace it's
///     just `"auto_refresh"`.
///
/// [`crate::Scheduler::cancel_namespace`] uses `namespace.starts_with(prefix)`
/// to scope shutdowns ("unload plugin X" cancels every `("plugin", "x:*")`
/// without touching the marketplace task).
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleKey {
    pub namespace: String,
    pub name:      String,
}

impl ScheduleKey {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self { namespace: namespace.into(), name: name.into() }
    }
}

impl fmt::Display for ScheduleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.namespace, self.name)
    }
}

impl<A: Into<String>, B: Into<String>> From<(A, B)> for ScheduleKey {
    fn from((a, b): (A, B)) -> Self {
        Self::new(a, b)
    }
}
