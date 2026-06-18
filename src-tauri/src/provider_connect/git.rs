//! Git-host connectors — GitHub + GitLab (gitlab.com).
//!
//! Each connector owns its own `Arc<dyn GitProvider>`, assembled exactly the
//! way `lib.rs` seeds the `GitProviderRegistry` (a `GithubProvider` /
//! `GitlabProvider` over the shell's `session::*SessionProvider`). That keeps
//! the connector constructible WITHOUT `AppState` — it only needs the provider
//! for `descriptor()` + `auth_status()` (has_token + current_user). connect /
//! OAuth / disconnect bridge the existing `git_provider::oauth::*` glue.
//!
//! Both providers are OAuth-only in settings (no `Fields` method), so
//! `connect_fields` returns `Err` for any method — the FE won't call it for
//! them. Self-hosted GitLab (PAT) is connected via the legacy generic
//! `save_credential` command and is out of scope for these two connectors.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

use arbor_ipc::prelude::EventSink;
use corvus_git_provider_api::prelude::GitProvider;
use corvus_provider_descriptor::prelude::{AuthStatus, OAuthStart, ProviderDescriptor};

use crate::error::AppError;
use crate::git_provider::session::{GithubSessionProvider, GitlabSessionProvider};
use crate::git_provider::{GithubProvider, GitlabProvider};
use crate::provider_connect::{compose_git_auth_status, ConnectorRegistry, ProviderConnector};

/// Map a `ProviderError` onto `AppError` the same way `commands::auth_commands::pe`
/// does — preserving the message via `Display`.
fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

/// Pull a required field out of the `connect_fields` map.
fn field<'a>(fields: &'a HashMap<String, String>, key: &str) -> Result<&'a str, AppError> {
    fields
        .get(key)
        .map(|s| s.as_str())
        .ok_or_else(|| AppError::Other(format!("missing field: {key}")))
}

/// Save a Git-over-HTTPS credential for `host` — the same two writes the legacy
/// `save_credential` + `save_default_credential` commands perform (host-scoped
/// + default-for-host), so fetch/push pick it up.
fn save_git_credential(host: &str, username: &str, password: &str) -> Result<(), AppError> {
    crate::auth::credential_store::save(host, username, password)?;
    crate::auth::credential_store::save_for_host(host, username, password)?;
    Ok(())
}

// ── GitHub ────────────────────────────────────────────────────────────────────

/// GitHub connector — OAuth Device Authorization Grant only.
pub struct GithubConnector {
    provider: Arc<dyn GitProvider>,
}

impl GithubConnector {
    fn new() -> Self {
        Self {
            provider: Arc::new(GithubProvider::new(
                Arc::new(GithubSessionProvider::new()),
                "github.com",
            )),
        }
    }
}

#[async_trait]
impl ProviderConnector for GithubConnector {
    fn id(&self) -> &'static str {
        "github"
    }

    fn descriptor(&self) -> ProviderDescriptor {
        self.provider.descriptor()
    }

    async fn auth_status(&self) -> AuthStatus {
        // github.com is a fixed endpoint → no account_label. Connection is
        // always OAuth (Device Flow) when a token is present.
        compose_git_auth_status(self.provider.as_ref(), None, Some("oauth".into())).await
    }

    async fn connect_fields(
        &self,
        method_id: &str,
        fields: HashMap<String, String>,
    ) -> Result<(), AppError> {
        // GitHub host is fixed. PAT is saved as the `oauth:<token>` HTTP-Basic
        // pair the old GitSection used; username+password saves a raw pair.
        match method_id {
            "pat" => save_git_credential("github.com", "oauth", field(&fields, "token")?),
            "userpass" => {
                save_git_credential("github.com", field(&fields, "username")?, field(&fields, "password")?)
            }
            other => Err(AppError::Other(format!("github: unknown fields method '{other}'"))),
        }
    }

    async fn start_oauth(
        &self,
        method_id: &str,
        sink: Arc<dyn EventSink>,
    ) -> Result<OAuthStart, AppError> {
        match method_id {
            "oauth" => {
                let info = crate::git_provider::oauth::github::start(sink).await.map_err(pe)?;
                Ok(OAuthStart::Device {
                    user_code:        info.user_code,
                    verification_uri: info.verification_uri,
                    expires_in:       info.expires_in,
                    interval:         info.interval,
                })
            }
            other => Err(AppError::Other(format!("github: unknown oauth method '{other}'"))),
        }
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        crate::git_provider::oauth::github_flow::disconnect()
    }
}

// ── GitLab ────────────────────────────────────────────────────────────────────

/// GitLab connector — gitlab.com, OAuth Authorization Code + PKCE only.
/// Self-hosted instances (PAT) use the legacy `save_credential` path.
pub struct GitlabConnector {
    provider: Arc<dyn GitProvider>,
}

impl GitlabConnector {
    fn new() -> Self {
        Self {
            provider: Arc::new(GitlabProvider::new(Arc::new(GitlabSessionProvider::new()))),
        }
    }
}

#[async_trait]
impl ProviderConnector for GitlabConnector {
    fn id(&self) -> &'static str {
        "gitlab"
    }

    fn descriptor(&self) -> ProviderDescriptor {
        self.provider.descriptor()
    }

    async fn auth_status(&self) -> AuthStatus {
        // gitlab.com is a fixed endpoint → no account_label; OAuth when authed.
        compose_git_auth_status(self.provider.as_ref(), None, Some("oauth".into())).await
    }

    async fn connect_fields(
        &self,
        method_id: &str,
        fields: HashMap<String, String>,
    ) -> Result<(), AppError> {
        // The optional `host` field targets a self-hosted instance; empty ⇒
        // gitlab.com. PAT uses the `oauth:<token>` HTTP-Basic pair.
        let host = fields
            .get("host")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("gitlab.com");
        match method_id {
            "pat" => save_git_credential(host, "oauth", field(&fields, "token")?),
            "userpass" => {
                save_git_credential(host, field(&fields, "username")?, field(&fields, "password")?)
            }
            other => Err(AppError::Other(format!("gitlab: unknown fields method '{other}'"))),
        }
    }

    async fn start_oauth(
        &self,
        method_id: &str,
        sink: Arc<dyn EventSink>,
    ) -> Result<OAuthStart, AppError> {
        match method_id {
            "oauth" => {
                let url = crate::git_provider::oauth::gitlab::start(sink).await.map_err(pe)?;
                Ok(OAuthStart::Redirect { url })
            }
            other => Err(AppError::Other(format!("gitlab: unknown oauth method '{other}'"))),
        }
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        crate::git_provider::oauth::gitlab_flow::disconnect()
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

static GIT_CONNECTORS: OnceLock<ConnectorRegistry> = OnceLock::new();

/// The process-wide git-host connector registry (GitHub + gitlab.com), built once.
pub fn git_connectors() -> &'static ConnectorRegistry {
    GIT_CONNECTORS.get_or_init(|| {
        let mut reg = ConnectorRegistry::new();
        reg.register(Box::new(GithubConnector::new()));
        reg.register(Box::new(GitlabConnector::new()));
        reg
    })
}
