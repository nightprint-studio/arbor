//! The SessionProvider-injected registry builder — the seam that lets the same
//! tracker code run in-process (vault) or out-of-process (reverse channel).

use std::sync::Arc;

use arbor_ipc::prelude::SessionProvider;
use corvus_issue_tracker_api::prelude::IssueTrackerRegistry;
use corvus_issue_tracker_jira::prelude::JiraTracker;
use corvus_issue_tracker_linear::prelude::LinearTracker;

/// Build the issue-tracker registry from a provider factory. `session_for(id)`
/// returns the `SessionProvider` for that provider id (`"linear"` | `"jira"`):
/// the shell passes one backed by `VaultSessionProvider`, `corvus-be` one backed
/// by `ChildSessionProvider`. The concrete `JiraTracker` handle is returned
/// alongside the registry because the Jira shim needs its inherent methods
/// (`download_attachment`, `current_user`) that aren't on the `IssueTracker`
/// trait.
///
/// The id literals (`"linear"`, `"jira"`) are the load-bearing routing keys: each
/// tracker stores its `account` and calls `session(account)` with it, and the
/// shell's `VaultSessionProvider::for_account` maps those same literals — keep
/// them identical, they are the wire contract between the two providers.
pub fn build_registry(
    session_for: impl Fn(&str) -> Arc<dyn SessionProvider>,
) -> (IssueTrackerRegistry, Arc<JiraTracker>) {
    let jira = Arc::new(JiraTracker::new(session_for("jira"), "jira"));
    let mut reg = IssueTrackerRegistry::new();
    reg.register(Arc::new(LinearTracker::new(session_for("linear"), "linear")));
    reg.register(jira.clone());
    (reg, jira)
}
