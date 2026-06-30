//! `reqwest` client pre-configured for Arbor.
//!
//! Centralizes the defaults that used to vary across modules
//! (timeouts of 0/20/30 seconds, user-agents of `"arbor-git-gui"` /
//! `"Arbor-Git-GUI/1.0"` / none) so the wire shape Arbor presents to
//! third-party APIs is uniform.

use std::time::Duration;

/// User-agent sent by every Arbor HTTP request that goes through this module.
/// Version tracks the workspace package version automatically.
pub const USER_AGENT: &str = concat!("Arbor-Git-GUI/", env!("CARGO_PKG_VERSION"));

/// Default request timeout. Long enough for slow GitLab self-hosted instances,
/// short enough that a hung connection doesn't wedge the UI thread waiting
/// on a join.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Pre-built [`reqwest::Client`] with the Arbor defaults.
///
/// Panics only if `reqwest` fails to construct a client from a fully-default
/// builder, which in practice means the TLS backend failed to initialise —
/// in that case the app is already unusable and a default fallback would
/// just defer the failure. Callers that need finer control (e.g. self-signed
/// cert acceptance for Jira Data Center) should use [`client_builder`] and
/// build the client themselves.
pub fn client() -> reqwest::Client {
    client_builder()
        .build()
        .expect("reqwest client with default config")
}

/// Pre-configured [`reqwest::ClientBuilder`] for further customization.
/// Already sets [`USER_AGENT`] and [`DEFAULT_TIMEOUT`]; the caller layers
/// on top.
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(DEFAULT_TIMEOUT)
}
