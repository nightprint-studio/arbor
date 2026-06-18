//! Issue-tracker connectors — Linear + Jira.
//!
//! Each `*Connector` is a thin [`ProviderConnector`] over the existing
//! `integrations::*` + `auth::oauth_*` glue: it forwards the domain trait's
//! `descriptor()`, maps the tracker-domain `auth_status()` onto the shared
//! shape, and bridges connect / OAuth / disconnect to the same functions the
//! legacy per-provider commands call. No new auth logic lives here.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

use arbor_ipc::prelude::EventSink;
use corvus_provider_descriptor::prelude::{AuthStatus, OAuthStart, ProviderDescriptor};

use crate::auth::{oauth_jira, oauth_linear};
use crate::error::AppError;
use crate::integrations::registry::registry;
use crate::provider_connect::{map_issue_auth_status, ConnectorRegistry, ProviderConnector};

/// Pull a required field out of the `connect_fields` map.
fn field<'a>(fields: &'a HashMap<String, String>, key: &str) -> Result<&'a str, AppError> {
    fields
        .get(key)
        .map(|s| s.as_str())
        .ok_or_else(|| AppError::Other(format!("missing field: {key}")))
}

// ── Linear ────────────────────────────────────────────────────────────────────

/// Linear connector. `pat` saves a Personal API Key (same as `linear_save_token`),
/// `oauth` starts the Authorization Code + PKCE flow.
pub struct LinearConnector;

#[async_trait]
impl ProviderConnector for LinearConnector {
    fn id(&self) -> &'static str {
        "linear"
    }

    fn descriptor(&self) -> ProviderDescriptor {
        registry()
            .get("linear")
            .expect("linear tracker is always registered")
            .descriptor()
    }

    async fn auth_status(&self) -> AuthStatus {
        let tracker = registry().get("linear").expect("linear tracker is always registered");
        match tracker.auth_status().await {
            Ok(s) => map_issue_auth_status(s),
            Err(_) => AuthStatus {
                authenticated: false,
                user:          None,
                account_label: None,
                method:        None,
            },
        }
    }

    async fn connect_fields(
        &self,
        method_id: &str,
        fields: HashMap<String, String>,
    ) -> Result<(), AppError> {
        match method_id {
            "pat" => {
                let token = field(&fields, "token")?;
                crate::integrations::linear::validate_and_save_token(token).await?;
                Ok(())
            }
            other => Err(AppError::Other(format!("linear: unknown fields method '{other}'"))),
        }
    }

    async fn start_oauth(
        &self,
        method_id: &str,
        sink: Arc<dyn EventSink>,
    ) -> Result<OAuthStart, AppError> {
        match method_id {
            "oauth" => {
                let url = oauth_linear::start_linear_oauth(sink).await?;
                Ok(OAuthStart::Redirect { url })
            }
            other => Err(AppError::Other(format!("linear: unknown oauth method '{other}'"))),
        }
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        oauth_linear::disconnect()
    }
}

// ── Jira ────────────────────────────────────────────────────────────────────

/// Jira connector. `basic` saves API-token / Basic-Auth credentials (same as
/// `jira_save_basic_auth`), `oauth` starts the Atlassian 3LO + PKCE flow.
pub struct JiraConnector;

#[async_trait]
impl ProviderConnector for JiraConnector {
    fn id(&self) -> &'static str {
        "jira"
    }

    fn descriptor(&self) -> ProviderDescriptor {
        registry()
            .get("jira")
            .expect("jira tracker is always registered")
            .descriptor()
    }

    async fn auth_status(&self) -> AuthStatus {
        let tracker = registry().get("jira").expect("jira tracker is always registered");
        match tracker.auth_status().await {
            Ok(s) => map_issue_auth_status(s),
            Err(_) => AuthStatus {
                authenticated: false,
                user:          None,
                account_label: None,
                method:        None,
            },
        }
    }

    async fn connect_fields(
        &self,
        method_id: &str,
        fields: HashMap<String, String>,
    ) -> Result<(), AppError> {
        match method_id {
            "basic" => {
                let domain = field(&fields, "domain")?;
                let api_token = field(&fields, "api_token")?;
                // Email is required only for Jira Cloud — the descriptor's
                // `required_when` rule enforces that on the FE; pass through
                // whatever was provided (empty is fine for Data Center/Server).
                let email = fields.get("email").map(|s| s.as_str()).unwrap_or("");
                crate::integrations::jira::validate_and_save_basic(email, api_token, domain).await?;
                Ok(())
            }
            other => Err(AppError::Other(format!("jira: unknown fields method '{other}'"))),
        }
    }

    async fn start_oauth(
        &self,
        method_id: &str,
        sink: Arc<dyn EventSink>,
    ) -> Result<OAuthStart, AppError> {
        match method_id {
            "oauth" => {
                let url = oauth_jira::start_jira_oauth(sink).await?;
                Ok(OAuthStart::Redirect { url })
            }
            other => Err(AppError::Other(format!("jira: unknown oauth method '{other}'"))),
        }
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        oauth_jira::disconnect()
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

static ISSUE_CONNECTORS: OnceLock<ConnectorRegistry> = OnceLock::new();

/// The process-wide issue-tracker connector registry (Linear + Jira), built once.
pub fn issue_connectors() -> &'static ConnectorRegistry {
    ISSUE_CONNECTORS.get_or_init(|| {
        let mut reg = ConnectorRegistry::new();
        reg.register(Box::new(LinearConnector));
        reg.register(Box::new(JiraConnector));
        reg
    })
}
