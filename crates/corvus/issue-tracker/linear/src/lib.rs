//! `corvus-issue-tracker-linear` — the Linear implementation of the Corvus
//! [`corvus_issue_tracker_api::prelude::IssueTracker`] trait.
//!
//! Keyring-free: the session (base URL + `Authorization` header) arrives through
//! an injected `Arc<dyn arbor_ipc::prelude::SessionProvider>`. The struct holds
//! an opaque `account` path (the shell maps it to the keyring), so only the
//! shell ever reaches the keyring.
//!
//! ## Public API: use the [`prelude`]

use std::sync::Arc;

use serde_json::{json, Value};

use arbor_ipc::prelude::{CredentialError, SessionProvider};
use corvus_issue_tracker_api::prelude::{IssueTrackerError, IssueUser, Result};

mod map;
mod tracker;
pub mod prelude;

/// Linear's GraphQL endpoint — the base the shell-side session provider returns.
pub const LINEAR_GQL: &str = "https://api.linear.app/graphql";

/// A Linear issue tracker bound to one account's injected credentials.
pub struct LinearTracker {
    session: Arc<dyn SessionProvider>,
    account: String,
    http:    reqwest::Client,
}

impl LinearTracker {
    /// Build a tracker. `account` is the opaque credential path the shell maps
    /// to the keyring; the base URL + auth header come from the session.
    pub fn new(session: Arc<dyn SessionProvider>, account: impl Into<String>) -> Self {
        Self { session, account: account.into(), http: arbor_core::prelude::client() }
    }

    /// GraphQL call against the injected session, transparently refreshing on a
    /// `401`. Linear's OAuth access token expires (~24h); the refresh token
    /// survives until the user revokes the app — without this retry every expiry
    /// would force a fresh OAuth dance.
    async fn gql_authed(&self, query: &str, variables: Value) -> Result<Value> {
        let session = self.session.session(&self.account).await.map_err(|e| match e {
            CredentialError::NotFound(_) => IssueTrackerError::NotConnected("Not connected to Linear".into()),
            other => IssueTrackerError::Auth(other.to_string()),
        })?;

        match raw_gql(&self.http, &session.base_url, &session.auth_header, query, variables.clone()).await {
            Err(IssueTrackerError::Auth(_)) => {
                // Silent refresh — failure (no refresh token, or provider
                // rejected it) surfaces the same "expired" message as before.
                let s2 = self
                    .session
                    .refresh(&self.account)
                    .await
                    .map_err(|_| IssueTrackerError::Auth("Invalid or expired Linear API key".into()))?;
                raw_gql(&self.http, &s2.base_url, &s2.auth_header, query, variables).await
            }
            other => other,
        }
    }
}

/// Validate a token before it's stored, returning the authenticated user.
/// Used by the connect flow (the shell saves the token only if this succeeds).
pub async fn validate_token(token: &str, base: &str) -> Result<IssueUser> {
    let http = arbor_core::prelude::client();
    let header = format!("Bearer {token}");
    let data = raw_gql(&http, base, &header, "{ viewer { id name displayName avatarUrl email } }", json!({})).await?;
    Ok(map::map_user(&data["viewer"]))
}

/// The shared HTTP round-trip behind [`LinearTracker::gql_authed`] and
/// [`validate_token`]. Error messages are preserved verbatim for the UI.
async fn raw_gql(
    http: &reqwest::Client,
    base: &str,
    auth_header: &str,
    query: &str,
    variables: Value,
) -> Result<Value> {
    let resp = http
        .post(base)
        .header("Authorization", auth_header)
        .header("Content-Type", "application/json")
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(|e| IssueTrackerError::Network(format!("Linear request failed: {e}")))?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(IssueTrackerError::Auth("Invalid or expired Linear API key".into()));
    }
    if !status.is_success() {
        return Err(IssueTrackerError::Api(format!("Linear API error: HTTP {status}")));
    }

    let parsed: Value = resp
        .json()
        .await
        .map_err(|e| IssueTrackerError::Api(format!("Linear JSON parse error: {e}")))?;

    if let Some(errors) = parsed.get("errors") {
        let msg = errors[0]["message"].as_str().unwrap_or("GraphQL error");
        return Err(IssueTrackerError::Api(format!("Linear: {msg}")));
    }

    Ok(parsed["data"].clone())
}
