//! `corvus-issue-tracker-jira` — the Jira (Cloud / Server / Data Center)
//! implementation of the Corvus [`corvus_issue_tracker_api::prelude::IssueTracker`]
//! trait.
//!
//! Keyring-free: the session (per-tenant base URL + `Authorization` header +
//! user-facing `web_base`) arrives through an injected
//! `Arc<dyn arbor_ipc::prelude::SessionProvider>`. The struct holds an opaque
//! `account` path; only the shell reaches the keyring or runs the OAuth refresh.
//!
//! ## Public API: use the [`prelude`]

use std::sync::Arc;

use arbor_ipc::prelude::SessionProvider;

mod http;
mod map;
mod tracker;
pub mod prelude;

/// A Jira issue tracker bound to one account's injected credentials.
pub struct JiraTracker {
    session: Arc<dyn SessionProvider>,
    account: String,
    http:    reqwest::Client,
}

impl JiraTracker {
    /// Build a tracker. `account` is the opaque credential path the shell maps
    /// to the keyring; the base URL + auth header + web base come from the
    /// session.
    pub fn new(session: Arc<dyn SessionProvider>, account: impl Into<String>) -> Self {
        Self { session, account: account.into(), http: jira_http_client() }
    }
}

/// A reqwest client that tolerates self-signed / internal-CA certs — many Jira
/// Data Center / Server installs sit behind those, and this is a desktop tool
/// talking to internal infrastructure.
fn jira_http_client() -> reqwest::Client {
    arbor_core::prelude::client_builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default()
}
