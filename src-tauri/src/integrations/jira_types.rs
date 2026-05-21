//! Jira-specific auth status type (extends the common IssueUser shape).

use serde::{Deserialize, Serialize};
use crate::integrations::IssueUser;

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
