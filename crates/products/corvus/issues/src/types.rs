//! Shared issue-tracker DTOs that aren't in the per-provider crates.

use serde::{Deserialize, Serialize};

use corvus_issue_tracker_api::prelude::IssueUser;

/// Jira-specific auth status (extends the common `IssueUser` shape with the
/// resolved host + auth method). Moved verbatim from the shell so both runtimes
/// return the identical wire shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraAuthStatus {
    pub authenticated: bool,
    pub user:          Option<IssueUser>,
    /// Human-readable Jira host, e.g. "mycompany.atlassian.net"
    pub domain:        Option<String>,
    /// "oauth" | "basic"
    pub auth_method:   Option<String>,
}
