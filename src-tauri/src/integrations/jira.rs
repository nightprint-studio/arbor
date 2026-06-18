//! Jira issue tracker — thin shell shim over `corvus-issue-tracker-jira`.
//!
//! The REST/ADF logic lives in the crate (keyring-free, credentials injected via
//! `SessionProvider`). Here we keep the keyring glue (`oauth_jira`) and adapt the
//! crate's `IssueTracker` (+ its Jira-specific inherent methods) to the existing
//! command surface, mapping `IssueTrackerError` → `AppError` byte-identically.

use std::path::Path;

use corvus_issues::prelude::{
    jira_new_issue, Issue, IssueComment, IssueFilterOptions, IssueFilters, IssueTracker, IssueUser,
};

use crate::auth::oauth_jira;
use crate::error::{AppError, Result};
use crate::integrations::jira_types::JiraAuthStatus;
use crate::integrations::registry::{jira_tracker, to_app_error};

// ── Auth ──────────────────────────────────────────────────────────────────────

/// Validate Basic Auth / PAT credentials (email + API token + domain) and return
/// the user. Saves to the keyring first, then validates via `/myself`.
pub async fn validate_and_save_basic(email: &str, api_token: &str, domain: &str) -> Result<IssueUser> {
    oauth_jira::save_basic_auth(email, api_token, domain)?;
    jira_tracker()
        .current_user()
        .await
        .map_err(|e| AppError::AuthFailed(format!("Jira /myself failed: {e}")))
}

/// Current auth status (authenticated flag + user + domain + method).
pub async fn get_auth_status() -> Result<JiraAuthStatus> {
    let Some(cfg) = oauth_jira::get_config()? else {
        return Ok(JiraAuthStatus { authenticated: false, user: None, domain: None, auth_method: None });
    };
    let status = jira_tracker().auth_status().await.map_err(to_app_error)?;
    if status.authenticated {
        Ok(JiraAuthStatus {
            authenticated: true,
            user:          status.user,
            domain:        cfg.domain,
            auth_method:   Some(cfg.auth_method),
        })
    } else {
        Ok(JiraAuthStatus { authenticated: false, user: None, domain: None, auth_method: None })
    }
}

// ── Operations (delegate to the crate) ────────────────────────────────────────

pub async fn search_issues(filters: IssueFilters) -> Result<Vec<Issue>> {
    jira_tracker().search_issues(filters).await.map_err(to_app_error)
}

pub async fn get_issue(key: &str) -> Result<Issue> {
    jira_tracker().get_issue(key).await.map_err(to_app_error)
}

pub async fn get_filter_options() -> Result<IssueFilterOptions> {
    jira_tracker().get_filter_options().await.map_err(to_app_error)
}

pub async fn transition_issue(key: &str, status_id: &str) -> Result<Issue> {
    jira_tracker().transition_issue(key, status_id).await.map_err(to_app_error)
}

pub async fn assign_issue(key: &str, account_id: Option<&str>) -> Result<Issue> {
    jira_tracker().assign_issue(key, account_id).await.map_err(to_app_error)
}

pub async fn add_comment(key: &str, body: &str) -> Result<IssueComment> {
    jira_tracker().add_comment(key, body).await.map_err(to_app_error)
}

pub async fn download_attachment(content_url: &str, dest_path: &Path) -> Result<u64> {
    jira_tracker().download_attachment(content_url, dest_path).await.map_err(to_app_error)
}

pub async fn fetch_image_bytes(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    jira_tracker().fetch_image_bytes(url).await.map_err(to_app_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_issue_req(
    title: &str,
    description: Option<&str>,
    team_id: &str,
    status_id: Option<&str>,
    assignee_id: Option<&str>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    _project_id: Option<&str>, // unused in Jira (mapped to team/project)
    milestone_id: Option<&str>,
    due_date: Option<&str>,
    estimate: Option<f64>,
    issue_type: Option<&str>,
) -> Result<Issue> {
    let req = jira_new_issue(
        title, description, team_id, status_id, assignee_id, label_ids, priority, milestone_id,
        due_date, estimate, issue_type,
    );
    jira_tracker().create_issue(req).await.map_err(to_app_error)
}
