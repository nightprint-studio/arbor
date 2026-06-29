//! Jira issue tracker — thin shell shim over `corvus-issue-tracker-jira`.
//!
//! The REST/ADF logic lives in the crate (keyring-free, credentials injected via
//! `SessionProvider`). What stays shell-side is the keyring glue (`oauth_jira`):
//! credential validation/storage, the auth-status read, and the host-gated
//! inline-image fetch. The issue *operations* (search/get/transition/assign/
//! comment/create/attachment) now flow through the `GitProvider`-style trait in
//! `corvus-be`, so their shell wrappers are gone.

// `IssueTracker` is the trait that brings `current_user` / `auth_status` /
// `fetch_image_bytes` into scope on the concrete `jira_tracker()` handle.
use corvus_issues::prelude::{IssueTracker, IssueUser};

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

// ── Inline-image proxy (host-gated, stays shell-side) ─────────────────────────

pub async fn fetch_image_bytes(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    jira_tracker().fetch_image_bytes(url).await.map_err(to_app_error)
}
