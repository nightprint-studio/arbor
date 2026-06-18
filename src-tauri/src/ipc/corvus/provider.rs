//! `provider` domain — generic, by-id provider-connection handlers routed
//! through the in-process broker.
//!
//! Two parallel command sets (issue trackers, git hosts) with an identical
//! shape: each dispatches to the [`ConnectorRegistry`](crate::provider_connect::ConnectorRegistry)
//! of its domain by provider `id`. The FE drives list / status / connect /
//! disconnect for ANY provider through these. Behavior (errors, async-ness) is
//! byte-identical to the former `#[tauri::command]`s.
//!
//! The `*_start_oauth` handlers begin an OAuth method and return an `OAuthStart`
//! telling the FE how to proceed; the spawned flow signals completion via the
//! unified `arbor://provider-oauth-done` event, emitted through the backend
//! [`EventSink`](arbor_ipc::prelude::EventSink) (`state.event_sink()`) — so they
//! run from the `&AppState` handler context with no `AppHandle`.

use std::collections::HashMap;

use corvus_provider_descriptor::prelude::{AuthStatus, OAuthStart, ProviderDescriptor};

use crate::error::AppError;
use crate::ipc::corvus;
use crate::provider_connect::git::git_connectors;
use crate::provider_connect::issue::issue_connectors;
use crate::provider_connect::ConnectorRegistry;
use crate::AppState;

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
#[corvus::handler]
async fn issue_provider_auth_status(_state: &AppState, id: String) -> Result<AuthStatus, AppError> {
    Ok(connector(issue_connectors(), &id)?.auth_status().await)
}

/// Save `Fields`-method credentials for an issue-tracker provider. `fields` keys
/// match the descriptor's `AuthField.key` (e.g. Linear `{ token }`, Jira
/// `{ domain, email?, api_token }`).
#[corvus::handler]
async fn issue_provider_connect_fields(
    _state: &AppState,
    id: String,
    method_id: String,
    fields: HashMap<String, String>,
) -> Result<(), AppError> {
    connector(issue_connectors(), &id)?
        .connect_fields(&method_id, fields)
        .await
}

/// Begin an OAuth method on an issue-tracker provider; the returned `OAuthStart`
/// tells the FE how to proceed. Completion arrives via `arbor://provider-oauth-done`.
#[corvus::handler]
async fn issue_provider_start_oauth(
    state: &AppState,
    id: String,
    method_id: String,
) -> Result<OAuthStart, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    connector(issue_connectors(), &id)?
        .start_oauth(&method_id, sink)
        .await
}

/// Remove all stored credentials for an issue-tracker provider.
#[corvus::handler]
async fn issue_provider_disconnect(_state: &AppState, id: String) -> Result<(), AppError> {
    connector(issue_connectors(), &id)?.disconnect().await
}

// ── Git-host domain ───────────────────────────────────────────────────────────

/// List the registered git-host providers with their self-describing connect
/// forms (id, icon, description, auth methods + fields). Mirrors
/// `list_issue_providers` for the git domain.
#[corvus::handler]
fn list_git_providers(_state: &AppState) -> Result<Vec<ProviderDescriptor>, AppError> {
    Ok(git_connectors().descriptors())
}

/// Current auth state of a git-host provider, composed from `has_token()` +
/// `current_user()`.
#[corvus::handler]
async fn git_provider_auth_status(_state: &AppState, id: String) -> Result<AuthStatus, AppError> {
    Ok(connector(git_connectors(), &id)?.auth_status().await)
}

/// Save `Fields`-method credentials for a git-host provider. github.com /
/// gitlab.com are OAuth-only, so this returns `Err` for them — the FE knows
/// from the descriptor not to call it.
#[corvus::handler]
async fn git_provider_connect_fields(
    _state: &AppState,
    id: String,
    method_id: String,
    fields: HashMap<String, String>,
) -> Result<(), AppError> {
    connector(git_connectors(), &id)?
        .connect_fields(&method_id, fields)
        .await
}

/// Begin an OAuth method on a git-host provider; the returned `OAuthStart` tells
/// the FE how to proceed (GitHub → Device, GitLab → Redirect). Completion
/// arrives via `arbor://provider-oauth-done`.
#[corvus::handler]
async fn git_provider_start_oauth(
    state: &AppState,
    id: String,
    method_id: String,
) -> Result<OAuthStart, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    connector(git_connectors(), &id)?
        .start_oauth(&method_id, sink)
        .await
}

/// Remove all stored credentials for a git-host provider.
#[corvus::handler]
async fn git_provider_disconnect(_state: &AppState, id: String) -> Result<(), AppError> {
    connector(git_connectors(), &id)?.disconnect().await
}
