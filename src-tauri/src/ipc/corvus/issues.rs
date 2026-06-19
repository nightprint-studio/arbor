//! `issues` domain — Linear / Jira issue-tracker handlers routed through the
//! in-process broker.
//!
//! These resolve **issue-tracker** providers (NOT git providers): each handler
//! delegates to the corresponding `crate::integrations::{linear,jira}` function,
//! which reads the connected tracker's credentials and performs a REST call.
//! Network-bound handlers are `async` (registered `Kind::Async`, awaited on the
//! runtime); the two pure helpers (`list_issue_providers`, `branch_name_for_issue`)
//! are sync. No plugin hooks are fired in this domain.

use crate::error::AppError;
use crate::integrations::jira_types::JiraAuthStatus;
use crate::integrations::{Issue, ProviderDescriptor};
use crate::ipc::corvus;
use crate::AppState;

/// List the registered issue-tracker providers with their self-describing
/// connect forms (id, icon, description, auth methods + fields). Drives the
/// generic settings UI.
#[corvus::handler]
fn list_issue_providers(_state: &AppState) -> Result<Vec<ProviderDescriptor>, AppError> {
    Ok(crate::integrations::registry::registry().descriptors())
}

/// Suggest a git branch name for an issue. Provider-agnostic — the helper
/// produces `{lower-identifier}-{slugified-title}` from any tracker's issue.
#[corvus::handler]
fn branch_name_for_issue(_state: &AppState, issue: Issue) -> Result<String, AppError> {
    Ok(crate::integrations::branch_name_for_issue(&issue))
}

// ── Jira ─────────────────────────────────────────────────────────────────────

#[corvus::handler]
async fn jira_get_auth_status(_state: &AppState) -> Result<JiraAuthStatus, AppError> {
    crate::integrations::jira::get_auth_status().await
}
