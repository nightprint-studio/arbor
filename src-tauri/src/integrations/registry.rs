//! The process-wide issue-tracker registry, plus error mapping.
//!
//! The registry construction + the trackers themselves live in the shared
//! `corvus-issues` crate; here we just inject the shell's keyring-backed
//! `VaultSessionProvider` and keep the `AppError` mapping (Tauri-specific). The
//! `corvus-be` process builds the same registry from `corvus_issues::build_registry`
//! injecting a `ChildSessionProvider` instead.

use std::sync::{Arc, OnceLock};

use arbor_ipc::prelude::SessionProvider;
use corvus_issues::prelude::{build_registry, IssueTrackerError, IssueTrackerRegistry, JiraTracker};

use crate::auth::vault::VaultSessionProvider;
use crate::error::AppError;

/// Built once: the registry **and** the shared concrete Jira handle, from a
/// single `OnceLock` (not two) so `registry()` and `jira_tracker()` return the
/// *same* `Arc<JiraTracker>` the registry registered — two locks would each call
/// `build_registry` and mint two trackers.
static ISSUES: OnceLock<(IssueTrackerRegistry, Arc<JiraTracker>)> = OnceLock::new();

fn issues() -> &'static (IssueTrackerRegistry, Arc<JiraTracker>) {
    ISSUES.get_or_init(|| {
        build_registry(|id| Arc::new(VaultSessionProvider::for_account(id)) as Arc<dyn SessionProvider>)
    })
}

/// The shared issue-tracker registry (lazily initialised on first use).
pub fn registry() -> &'static IssueTrackerRegistry {
    &issues().0
}

/// The shared Jira tracker (concrete handle, for the shim's inherent methods).
pub fn jira_tracker() -> Arc<JiraTracker> {
    issues().1.clone()
}

/// Map a tracker error onto `AppError`, preserving the message byte-identically:
/// auth/connection failures → `AuthFailed`, everything else → `Other`.
pub fn to_app_error(e: IssueTrackerError) -> AppError {
    match e {
        IssueTrackerError::Auth(m) | IssueTrackerError::NotConnected(m) => AppError::AuthFailed(m),
        IssueTrackerError::Api(m) | IssueTrackerError::Network(m) => AppError::Other(m),
    }
}
