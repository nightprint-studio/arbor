use std::sync::Arc;
use tauri::State;

use crate::error::AppError;
use crate::AppState;
use crate::git_provider::GitProvider;
use crate::git_provider::types::ProviderUser;

fn provider_by_host(state: &AppState, host: &str) -> Result<Arc<dyn GitProvider>, AppError> {
    let registry = state.lock_git_providers()?;
    registry.for_host(host)
        .ok_or_else(|| AppError::Other(format!("No GitProvider registered for host '{host}'")))
}

fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

// ── Credential store (username + password / PAT) ──────────────────────────

/// Save the "default" credential for a host — used automatically by fetch/push.
#[tauri::command]
pub fn save_default_credential(
    _state: State<'_, AppState>,
    url_or_host: String,
    username: String,
    password: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::save_for_host(&url_or_host, &username, &password)
}

/// Returns true if a default credential is stored for the given host/URL.
#[tauri::command]
pub fn has_default_credential(
    _state: State<'_, AppState>,
    url_or_host: String,
) -> Result<bool, AppError> {
    Ok(crate::auth::credential_store::get_for_host(&url_or_host)?.is_some())
}

/// Delete the default credential for a host/URL.
#[tauri::command]
pub fn delete_default_credential(
    _state: State<'_, AppState>,
    url_or_host: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::delete_for_host(&url_or_host)
}

#[tauri::command]
pub fn save_credential(
    _state: State<'_, AppState>,
    host: String,
    username: String,
    password: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::save(&host, &username, &password)
}

#[tauri::command]
pub fn get_credential(
    _state: State<'_, AppState>,
    host: String,
    username: String,
) -> Result<Option<String>, AppError> {
    crate::auth::credential_store::get(&host, &username)
}

#[tauri::command]
pub fn delete_credential(
    _state: State<'_, AppState>,
    host: String,
    username: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::delete(&host, &username)
}

// ── GitHub OAuth (Device Authorization Grant — RFC 8628) ────────────────────

/// Start the GitHub Device Authorization Grant flow.
///
/// Returns `DeviceFlowInfo` (`user_code`, `verification_uri`, `expires_in`,
/// `interval`) for the UI to display.  Listen for the
/// `arbor://github-oauth-done` Tauri event (payload: `null` on success, error
/// string on failure) to know when the user completes authorisation.
#[tauri::command]
pub async fn start_github_device_flow(
    _state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<crate::auth::DeviceFlowInfo, AppError> {
    // The provider trait can't carry the AppHandle needed by the listener;
    // use the dedicated helper that the trait method itself delegates to.
    crate::git_provider::oauth::github::start(app_handle).await.map_err(pe)
}

#[tauri::command]
pub fn get_github_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(provider_by_host(&state, "github.com")?.has_token())
}

/// Fetch the authenticated GitHub user (login, name, email, avatar).
/// Returns `None` if no token is stored or the request fails — the settings
/// UI uses this to render a user badge and shouldn't surface transient errors.
#[tauri::command]
pub async fn get_github_user(state: State<'_, AppState>) -> Result<Option<ProviderUser>, AppError> {
    let provider = provider_by_host(&state, "github.com")?;
    if !provider.has_token() { return Ok(None); }
    Ok(provider.current_user().await.ok())
}

#[tauri::command]
pub async fn disconnect_github(state: State<'_, AppState>) -> Result<(), AppError> {
    let provider = provider_by_host(&state, "github.com")?;
    provider.revoke_token().await.map_err(pe)
}

/// Attempt to silently refresh the stored GitHub OAuth access token.
/// Returns `true` when a new token was obtained; `false` when no refresh token
/// is stored (non-expiring token, PAT, or not yet authed via OAuth).
#[tauri::command]
pub async fn try_refresh_github_token(
    _state: State<'_, AppState>,
) -> Result<bool, AppError> {
    crate::git_provider::oauth::github_flow::try_refresh().await
}

// ── GitLab OAuth (Authorization Code + PKCE) ────────────────────────────────

/// Start the GitLab OAuth flow. Returns the authorization URL to open in the browser.
/// Listen for the `arbor://gitlab-oauth-done` Tauri event (payload: bool) when done.
#[tauri::command]
pub async fn start_gitlab_oauth(
    _state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, AppError> {
    crate::git_provider::oauth::gitlab::start(app_handle).await.map_err(pe)
}

#[tauri::command]
pub fn get_gitlab_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(provider_by_host(&state, "gitlab.com")?.has_token())
}

/// Fetch the authenticated GitLab user (login, name, email, avatar).
/// Returns `None` if no token is stored or the request fails.
#[tauri::command]
pub async fn get_gitlab_user(state: State<'_, AppState>) -> Result<Option<ProviderUser>, AppError> {
    let provider = provider_by_host(&state, "gitlab.com")?;
    if !provider.has_token() { return Ok(None); }
    Ok(provider.current_user().await.ok())
}

#[tauri::command]
pub async fn disconnect_gitlab(state: State<'_, AppState>) -> Result<(), AppError> {
    let provider = provider_by_host(&state, "gitlab.com")?;
    provider.revoke_token().await.map_err(pe)
}

/// Attempt to silently refresh the stored GitLab OAuth access token.
/// Returns `true` when a new token was obtained; `false` when no refresh token is stored.
#[tauri::command]
pub async fn try_refresh_gitlab_token(
    _state: State<'_, AppState>,
) -> Result<bool, AppError> {
    crate::git_provider::oauth::gitlab_flow::try_refresh().await
}

// ── Linear OAuth (Authorization Code + PKCE) ────────────────────────────────

/// Start the Linear OAuth flow. Returns the authorization URL.
/// The frontend should open the URL in the browser, then listen for the
/// `arbor://linear-oauth-done` Tauri event (payload: bool).
#[tauri::command]
pub async fn start_linear_oauth(
    _state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, AppError> {
    crate::auth::oauth_linear::start_linear_oauth(app_handle).await
}

/// Returns `true` if a Linear token (PAT or OAuth) is present in the keychain.
#[tauri::command]
pub fn get_linear_oauth_status(_state: State<'_, AppState>) -> Result<bool, AppError> {
    crate::auth::oauth_linear::get_status()
}

/// Remove the Linear token from the keychain (works for both PAT and OAuth tokens).
#[tauri::command]
pub fn disconnect_linear_oauth(_state: State<'_, AppState>) -> Result<(), AppError> {
    crate::auth::oauth_linear::disconnect()
}

/// Attempt to silently refresh the stored Linear OAuth access token.
/// Returns `true` when a new token was obtained; `false` when no refresh token
/// is stored (PAT auth flow) or refresh is not needed.
#[tauri::command]
pub async fn try_refresh_linear_token(
    _state: State<'_, AppState>,
) -> Result<bool, AppError> {
    crate::auth::oauth_linear::try_refresh().await
}

// ── Jira OAuth (Authorization Code + PKCE) ──────────────────────────────────

/// Start the Jira OAuth 2.0 (3LO) + PKCE flow.
/// Returns the authorization URL. Listen for `arbor://jira-oauth-done` (bool) when done.
#[tauri::command]
pub async fn start_jira_oauth(
    _state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, AppError> {
    crate::auth::oauth_jira::start_jira_oauth(app_handle).await
}

/// Returns `true` if Jira credentials (OAuth or Basic) are present in the keychain.
#[tauri::command]
pub fn get_jira_oauth_status(_state: State<'_, AppState>) -> Result<bool, AppError> {
    crate::auth::oauth_jira::get_status()
}

/// Remove all Jira credentials from the keychain.
#[tauri::command]
pub fn disconnect_jira(_state: State<'_, AppState>) -> Result<(), AppError> {
    crate::auth::oauth_jira::disconnect()
}

/// Attempt to refresh the Jira OAuth access token.
#[tauri::command]
pub async fn try_refresh_jira_token(
    _state: State<'_, AppState>,
) -> Result<bool, AppError> {
    crate::auth::oauth_jira::try_refresh().await
}
