//! Generic, by-id provider-connection IPC.
//!
//! Two parallel command sets — one per domain (issue trackers, git hosts) —
//! dispatch to the [`ConnectorRegistry`](crate::provider_connect::ConnectorRegistry)
//! of that domain by provider `id`. The FE drives connect / disconnect / OAuth
//! for ANY provider through these, with zero per-provider code: it lists
//! descriptors, renders the connect form the descriptor describes, then calls
//! the matching generic command.
//!
//! This surface is additive — the legacy per-provider commands
//! (`linear_save_token`, `jira_save_basic_auth`, `start_github_device_flow`, …)
//! stay until the FE finishes migrating.
//!
//! OAuth completion is signalled out-of-band by the unified event
//! `arbor://provider-oauth-done` with payload `{ id, ok, error }`.

use std::collections::HashMap;

use tauri::AppHandle;

use corvus_provider_descriptor::prelude::{AuthStatus, OAuthStart, ProviderDescriptor};

use crate::error::AppError;
use crate::provider_connect::git::git_connectors;
use crate::provider_connect::issue::issue_connectors;
use crate::provider_connect::ConnectorRegistry;

/// Resolve a connector by id within a domain registry, or a uniform "unknown
/// provider" error the FE can surface verbatim.
fn connector<'a>(
    reg: &'a ConnectorRegistry,
    id: &str,
) -> Result<&'a dyn crate::provider_connect::ProviderConnector, AppError> {
    reg.get(id)
        .ok_or_else(|| AppError::Other(format!("unknown provider '{id}'")))
}

// ── Issue-tracker domain ──────────────────────────────────────────────────────

/// Current auth state of an issue-tracker provider, mapped onto the shared shape.
#[tauri::command]
pub async fn issue_provider_auth_status(id: String) -> Result<AuthStatus, AppError> {
    Ok(connector(issue_connectors(), &id)?.auth_status().await)
}

/// Save `Fields`-method credentials for an issue-tracker provider. `fields` keys
/// match the descriptor's `AuthField.key` (e.g. Linear `{ token }`, Jira
/// `{ domain, email?, api_token }`).
#[tauri::command]
pub async fn issue_provider_connect_fields(
    id: String,
    method_id: String,
    fields: HashMap<String, String>,
) -> Result<(), AppError> {
    connector(issue_connectors(), &id)?
        .connect_fields(&method_id, fields)
        .await
}

/// Begin an OAuth method on an issue-tracker provider; the returned [`OAuthStart`]
/// tells the FE how to proceed. Completion arrives via `arbor://provider-oauth-done`.
#[tauri::command]
pub async fn issue_provider_start_oauth(
    id: String,
    method_id: String,
    app: AppHandle,
) -> Result<OAuthStart, AppError> {
    connector(issue_connectors(), &id)?
        .start_oauth(&method_id, app)
        .await
}

/// Remove all stored credentials for an issue-tracker provider.
#[tauri::command]
pub async fn issue_provider_disconnect(id: String) -> Result<(), AppError> {
    connector(issue_connectors(), &id)?.disconnect().await
}

// ── Git-host domain ───────────────────────────────────────────────────────────

/// List the registered git-host providers with their self-describing connect
/// forms (id, icon, description, auth methods + fields). Mirrors
/// `list_issue_providers` for the git domain.
#[tauri::command]
pub fn list_git_providers() -> Result<Vec<ProviderDescriptor>, AppError> {
    Ok(git_connectors().descriptors())
}

/// Current auth state of a git-host provider, composed from `has_token()` +
/// `current_user()`.
#[tauri::command]
pub async fn git_provider_auth_status(id: String) -> Result<AuthStatus, AppError> {
    Ok(connector(git_connectors(), &id)?.auth_status().await)
}

/// Save `Fields`-method credentials for a git-host provider. github.com /
/// gitlab.com are OAuth-only, so this returns `Err` for them — the FE knows
/// from the descriptor not to call it.
#[tauri::command]
pub async fn git_provider_connect_fields(
    id: String,
    method_id: String,
    fields: HashMap<String, String>,
) -> Result<(), AppError> {
    connector(git_connectors(), &id)?
        .connect_fields(&method_id, fields)
        .await
}

/// Begin an OAuth method on a git-host provider; the returned [`OAuthStart`]
/// tells the FE how to proceed (GitHub → Device, GitLab → Redirect). Completion
/// arrives via `arbor://provider-oauth-done`.
#[tauri::command]
pub async fn git_provider_start_oauth(
    id: String,
    method_id: String,
    app: AppHandle,
) -> Result<OAuthStart, AppError> {
    connector(git_connectors(), &id)?
        .start_oauth(&method_id, app)
        .await
}

/// Remove all stored credentials for a git-host provider.
#[tauri::command]
pub async fn git_provider_disconnect(id: String) -> Result<(), AppError> {
    connector(git_connectors(), &id)?.disconnect().await
}
