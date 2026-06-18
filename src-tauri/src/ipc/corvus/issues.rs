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
use crate::integrations::{
    Issue, IssueComment, IssueFilterOptions, IssueFilters, LinearAuthStatus,
    ProviderDescriptor,
};
use crate::ipc::corvus;
use crate::AppState;

/// List the registered issue-tracker providers with their self-describing
/// connect forms (id, icon, description, auth methods + fields). Drives the
/// generic settings UI.
#[corvus::handler]
fn list_issue_providers(_state: &AppState) -> Result<Vec<ProviderDescriptor>, AppError> {
    Ok(crate::integrations::registry::registry().descriptors())
}

#[corvus::handler]
async fn linear_get_auth_status(_state: &AppState) -> Result<LinearAuthStatus, AppError> {
    crate::integrations::linear::get_auth_status().await
}

#[corvus::handler]
async fn linear_search_issues(
    _state: &AppState,
    filters: IssueFilters,
) -> Result<Vec<Issue>, AppError> {
    crate::integrations::linear::search_issues(filters).await
}

#[corvus::handler]
async fn linear_get_issue(_state: &AppState, id: String) -> Result<Issue, AppError> {
    crate::integrations::linear::get_issue(&id).await
}

#[corvus::handler]
async fn linear_get_filter_options(_state: &AppState) -> Result<IssueFilterOptions, AppError> {
    crate::integrations::linear::get_filter_options().await
}

#[corvus::handler]
async fn linear_transition_issue(
    _state: &AppState,
    id: String,
    status_id: String,
) -> Result<Issue, AppError> {
    crate::integrations::linear::transition_issue(&id, &status_id).await
}

#[corvus::handler]
async fn linear_assign_issue(
    _state: &AppState,
    id: String,
    user_id: Option<String>,
) -> Result<Issue, AppError> {
    crate::integrations::linear::assign_issue(&id, user_id.as_deref()).await
}

#[corvus::handler]
async fn linear_add_comment(
    _state: &AppState,
    issue_id: String,
    body: String,
) -> Result<IssueComment, AppError> {
    crate::integrations::linear::add_comment(&issue_id, &body).await
}

#[corvus::handler]
async fn linear_create_issue(
    _state: &AppState,
    title: String,
    description: Option<String>,
    team_id: String,
    status_id: Option<String>,
    assignee_id: Option<String>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    project_id: Option<String>,
    milestone_id: Option<String>,
    due_date: Option<String>,
    estimate: Option<f64>,
) -> Result<Issue, AppError> {
    crate::integrations::linear::create_issue_req(
        &title,
        description.as_deref(),
        &team_id,
        status_id.as_deref(),
        assignee_id.as_deref(),
        label_ids,
        priority,
        project_id.as_deref(),
        milestone_id.as_deref(),
        due_date.as_deref(),
        estimate,
    )
    .await
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

#[corvus::handler]
async fn jira_search_issues(
    _state: &AppState,
    filters: IssueFilters,
) -> Result<Vec<Issue>, AppError> {
    crate::integrations::jira::search_issues(filters).await
}

#[corvus::handler]
async fn jira_get_issue(_state: &AppState, id: String) -> Result<Issue, AppError> {
    crate::integrations::jira::get_issue(&id).await
}

#[corvus::handler]
async fn jira_get_filter_options(_state: &AppState) -> Result<IssueFilterOptions, AppError> {
    crate::integrations::jira::get_filter_options().await
}

#[corvus::handler]
async fn jira_transition_issue(
    _state: &AppState,
    id: String,
    status_id: String,
) -> Result<Issue, AppError> {
    crate::integrations::jira::transition_issue(&id, &status_id).await
}

#[corvus::handler]
async fn jira_assign_issue(
    _state: &AppState,
    id: String,
    user_id: Option<String>,
) -> Result<Issue, AppError> {
    crate::integrations::jira::assign_issue(&id, user_id.as_deref()).await
}

#[corvus::handler]
async fn jira_add_comment(
    _state: &AppState,
    issue_id: String,
    body: String,
) -> Result<IssueComment, AppError> {
    crate::integrations::jira::add_comment(&issue_id, &body).await
}

#[corvus::handler]
async fn jira_create_issue(
    _state: &AppState,
    title: String,
    description: Option<String>,
    team_id: String,
    status_id: Option<String>,
    assignee_id: Option<String>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    project_id: Option<String>,
    milestone_id: Option<String>,
    due_date: Option<String>,
    estimate: Option<f64>,
    issue_type: Option<String>,
) -> Result<Issue, AppError> {
    crate::integrations::jira::create_issue_req(
        &title,
        description.as_deref(),
        &team_id,
        status_id.as_deref(),
        assignee_id.as_deref(),
        label_ids,
        priority,
        project_id.as_deref(),
        milestone_id.as_deref(),
        due_date.as_deref(),
        estimate,
        issue_type.as_deref(),
    )
    .await
}

/// Download a Jira attachment to `dest_path` (chosen by the frontend via the
/// save dialog). Returns the byte size written. The download URL must point at
/// the configured Jira host — see `jira::download_attachment`.
#[corvus::handler]
async fn jira_download_attachment(
    _state: &AppState,
    content_url: String,
    dest_path: String,
) -> Result<u64, AppError> {
    crate::integrations::jira::download_attachment(
        &content_url,
        std::path::Path::new(&dest_path),
    )
    .await
}
