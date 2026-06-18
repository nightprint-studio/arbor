//! Linear issue tracker — thin shell shim over `corvus-issue-tracker-linear`.
//!
//! The GraphQL/HTTP logic lives in the crate (keyring-free, credentials
//! injected via `SessionProvider`). Here we keep only the keyring glue (token
//! storage) and adapt the crate's `IssueTracker` to the existing command
//! surface, mapping `IssueTrackerError` → `AppError` byte-identically.

use std::sync::Arc;

use corvus_issues::prelude::{
    linear_new_issue, validate_token, Issue, IssueComment, IssueFilterOptions, IssueFilters,
    IssueTracker, IssueUser, LinearAuthStatus, LINEAR_GQL,
};

use crate::auth::credential_store;
use crate::error::Result;
use crate::integrations::registry::{registry, to_app_error};

const KEYRING_HOST: &str = "linear.app";
const KEYRING_USER: &str = "api-key";

/// The registered Linear tracker (always present once the registry is built).
fn tracker() -> Arc<dyn IssueTracker> {
    registry().get("linear").expect("linear tracker is always registered")
}

// ── Token storage (keyring — stays shell-side) ────────────────────────────────

fn save_token(token: &str) -> Result<()> {
    credential_store::save(KEYRING_HOST, KEYRING_USER, token)
}

// ── Auth ──────────────────────────────────────────────────────────────────────

pub async fn validate_and_save_token(token: &str) -> Result<IssueUser> {
    let user = validate_token(token, LINEAR_GQL).await.map_err(to_app_error)?;
    save_token(token)?;
    Ok(user)
}

pub async fn get_auth_status() -> Result<LinearAuthStatus> {
    let status = tracker().auth_status().await.map_err(to_app_error)?;
    Ok(LinearAuthStatus { authenticated: status.authenticated, user: status.user })
}

// ── Operations (delegate to the crate) ────────────────────────────────────────

pub async fn search_issues(filters: IssueFilters) -> Result<Vec<Issue>> {
    tracker().search_issues(filters).await.map_err(to_app_error)
}

pub async fn get_issue(id: &str) -> Result<Issue> {
    tracker().get_issue(id).await.map_err(to_app_error)
}

pub async fn get_filter_options() -> Result<IssueFilterOptions> {
    tracker().get_filter_options().await.map_err(to_app_error)
}

pub async fn transition_issue(id: &str, status_id: &str) -> Result<Issue> {
    tracker().transition_issue(id, status_id).await.map_err(to_app_error)
}

pub async fn assign_issue(id: &str, user_id: Option<&str>) -> Result<Issue> {
    tracker().assign_issue(id, user_id).await.map_err(to_app_error)
}

pub async fn add_comment(issue_id: &str, body: &str) -> Result<IssueComment> {
    tracker().add_comment(issue_id, body).await.map_err(to_app_error)
}

pub async fn fetch_image_bytes(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    tracker().fetch_image_bytes(url).await.map_err(to_app_error)
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
    project_id: Option<&str>,
    milestone_id: Option<&str>,
    due_date: Option<&str>,
    estimate: Option<f64>,
) -> Result<Issue> {
    let req = linear_new_issue(
        title, description, team_id, status_id, assignee_id, label_ids, priority, project_id,
        milestone_id, due_date, estimate,
    );
    tracker().create_issue(req).await.map_err(to_app_error)
}
