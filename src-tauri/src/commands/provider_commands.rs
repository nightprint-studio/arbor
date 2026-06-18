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
//!
//! Most of these moved to the Model-D handlers in `ipc::corvus::provider`. The
//! two `*_start_oauth` commands stay here: they take a `tauri::AppHandle` and
//! pass it into `ProviderConnector::start_oauth`, which spawns the OAuth flow
//! that emits the completion event through that handle — not reachable from the
//! `&AppState`-only handler context.

use tauri::AppHandle;

use corvus_provider_descriptor::prelude::OAuthStart;

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

// ── Git-host domain ───────────────────────────────────────────────────────────

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
