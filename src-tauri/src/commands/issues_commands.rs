use tauri::State;

use crate::AppState;
use crate::error::AppError;
use crate::integrations::{
    Issue, IssueComment, IssueFilterOptions, IssueFilters, IssueUser, LinearAuthStatus,
};
use crate::integrations::jira_types::JiraAuthStatus;

#[tauri::command]
pub async fn linear_get_auth_status(
    _state: State<'_, AppState>,
) -> Result<LinearAuthStatus, AppError> {
    crate::integrations::linear::get_auth_status().await
}

#[tauri::command]
pub async fn linear_save_token(
    _state: State<'_, AppState>,
    token: String,
) -> Result<IssueUser, AppError> {
    crate::integrations::linear::validate_and_save_token(&token).await
}

#[tauri::command]
pub async fn linear_logout(
    _state: State<'_, AppState>,
) -> Result<(), AppError> {
    crate::integrations::linear::delete_token()
}

#[tauri::command]
pub async fn linear_search_issues(
    _state: State<'_, AppState>,
    filters: IssueFilters,
) -> Result<Vec<Issue>, AppError> {
    crate::integrations::linear::search_issues(filters).await
}

#[tauri::command]
pub async fn linear_get_issue(
    _state: State<'_, AppState>,
    id: String,
) -> Result<Issue, AppError> {
    crate::integrations::linear::get_issue(&id).await
}

#[tauri::command]
pub async fn linear_get_filter_options(
    _state: State<'_, AppState>,
) -> Result<IssueFilterOptions, AppError> {
    crate::integrations::linear::get_filter_options().await
}

#[tauri::command]
pub async fn linear_transition_issue(
    _state: State<'_, AppState>,
    id: String,
    status_id: String,
) -> Result<Issue, AppError> {
    crate::integrations::linear::transition_issue(&id, &status_id).await
}

#[tauri::command]
pub async fn linear_assign_issue(
    _state: State<'_, AppState>,
    id: String,
    user_id: Option<String>,
) -> Result<Issue, AppError> {
    crate::integrations::linear::assign_issue(&id, user_id.as_deref()).await
}

#[tauri::command]
pub async fn linear_add_comment(
    _state: State<'_, AppState>,
    issue_id: String,
    body: String,
) -> Result<IssueComment, AppError> {
    crate::integrations::linear::add_comment(&issue_id, &body).await
}

#[tauri::command]
pub async fn linear_create_issue(
    _state: State<'_, AppState>,
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
    ).await
}

/// Suggest a git branch name for an issue. Provider-agnostic — the helper
/// produces `{lower-identifier}-{slugified-title}` from any tracker's issue.
#[tauri::command]
pub fn branch_name_for_issue(
    _state: State<'_, AppState>,
    issue: Issue,
) -> Result<String, AppError> {
    Ok(crate::integrations::branch_name_for_issue(&issue))
}

// ── Jira ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn jira_get_auth_status(
    _state: State<'_, AppState>,
) -> Result<JiraAuthStatus, AppError> {
    crate::integrations::jira::get_auth_status().await
}

#[tauri::command]
pub async fn jira_save_basic_auth(
    _state: State<'_, AppState>,
    email: String,
    api_token: String,
    domain: String,
) -> Result<IssueUser, AppError> {
    crate::integrations::jira::validate_and_save_basic(&email, &api_token, &domain).await
}

#[tauri::command]
pub async fn jira_logout(
    _state: State<'_, AppState>,
) -> Result<(), AppError> {
    crate::integrations::jira::delete_credentials()
}

#[tauri::command]
pub async fn jira_search_issues(
    _state: State<'_, AppState>,
    filters: IssueFilters,
) -> Result<Vec<Issue>, AppError> {
    crate::integrations::jira::search_issues(filters).await
}

#[tauri::command]
pub async fn jira_get_issue(
    _state: State<'_, AppState>,
    id: String,
) -> Result<Issue, AppError> {
    crate::integrations::jira::get_issue(&id).await
}

#[tauri::command]
pub async fn jira_get_filter_options(
    _state: State<'_, AppState>,
) -> Result<IssueFilterOptions, AppError> {
    crate::integrations::jira::get_filter_options().await
}

#[tauri::command]
pub async fn jira_transition_issue(
    _state: State<'_, AppState>,
    id: String,
    status_id: String,
) -> Result<Issue, AppError> {
    crate::integrations::jira::transition_issue(&id, &status_id).await
}

#[tauri::command]
pub async fn jira_assign_issue(
    _state: State<'_, AppState>,
    id: String,
    user_id: Option<String>,
) -> Result<Issue, AppError> {
    crate::integrations::jira::assign_issue(&id, user_id.as_deref()).await
}

#[tauri::command]
pub async fn jira_add_comment(
    _state: State<'_, AppState>,
    issue_id: String,
    body: String,
) -> Result<IssueComment, AppError> {
    crate::integrations::jira::add_comment(&issue_id, &body).await
}

#[tauri::command]
pub async fn jira_create_issue(
    _state: State<'_, AppState>,
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
    ).await
}

/// Download a Jira attachment to `dest_path` (chosen by the frontend via the
/// save dialog). Returns the byte size written. The download URL must point at
/// the configured Jira host — see `jira::download_attachment`.
#[tauri::command]
pub async fn jira_download_attachment(
    _state: State<'_, AppState>,
    content_url: String,
    dest_path: String,
) -> Result<u64, AppError> {
    crate::integrations::jira::download_attachment(
        &content_url,
        std::path::Path::new(&dest_path),
    ).await
}
