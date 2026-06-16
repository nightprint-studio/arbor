//! The process-wide issue-tracker registry, plus error mapping.
//!
//! Trackers are `Arc<dyn IssueTracker>` keyed by id. Built lazily. Adding a
//! provider is one `register` call. Jira also exposes its concrete handle
//! ([`jira_tracker`]) because its shim needs the Jira-specific inherent methods
//! (`download_attachment`, `current_user`) that aren't on the trait.

use std::sync::{Arc, OnceLock};

use corvus_issue_tracker_api::prelude::{IssueTrackerError, IssueTrackerRegistry};
use corvus_issue_tracker_jira::prelude::JiraTracker;
use corvus_issue_tracker_linear::prelude::LinearTracker;

use crate::error::AppError;
use crate::integrations::token_source::{JiraSessionProvider, LinearSessionProvider};

static REGISTRY: OnceLock<IssueTrackerRegistry> = OnceLock::new();
static JIRA: OnceLock<Arc<JiraTracker>> = OnceLock::new();

/// The shared Jira tracker (concrete handle, for the shim's inherent methods).
pub fn jira_tracker() -> Arc<JiraTracker> {
    JIRA.get_or_init(|| Arc::new(JiraTracker::new(Arc::new(JiraSessionProvider), "jira"))).clone()
}

/// The shared issue-tracker registry (lazily initialised on first use).
pub fn registry() -> &'static IssueTrackerRegistry {
    REGISTRY.get_or_init(|| {
        let mut reg = IssueTrackerRegistry::new();
        reg.register(Arc::new(LinearTracker::new(Arc::new(LinearSessionProvider), "linear")));
        reg.register(jira_tracker());
        reg
    })
}

/// Map a tracker error onto `AppError`, preserving the message byte-identically:
/// auth/connection failures → `AuthFailed`, everything else → `Other`.
pub fn to_app_error(e: IssueTrackerError) -> AppError {
    match e {
        IssueTrackerError::Auth(m) | IssueTrackerError::NotConnected(m) => AppError::AuthFailed(m),
        IssueTrackerError::Api(m) | IssueTrackerError::Network(m) => AppError::Other(m),
    }
}
