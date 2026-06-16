//! Shell-side generic provider-connection layer.
//!
//! A single `ProviderConnector` trait abstracts "connect any provider"
//! (issue tracker OR git host) behind a by-id surface, so the frontend can
//! drive every provider's connect / disconnect / OAuth flow through generic
//! IPC without a line of per-provider code. Each connector wraps the existing
//! per-provider functions (the keyring/OAuth glue that already lives in
//! `integrations/*` and `git_provider/oauth/*`); this layer adds NO new auth
//! logic, only a uniform shape.
//!
//! The provider's *self-description* — what the FE renders to list and connect
//! it — is the shared
//! [`ProviderDescriptor`](corvus_provider_descriptor::prelude::ProviderDescriptor),
//! already returned by each domain trait's `descriptor()`. Connectors forward
//! it verbatim.
//!
//! - Issue-tracker connectors live in [`issue`]; build via [`issue::issue_connectors`].
//! - Git-host connectors live in [`git`]; build via [`git::git_connectors`].

pub mod issue;
pub mod git;

use std::collections::HashMap;

use async_trait::async_trait;
use tauri::AppHandle;

use corvus_provider_descriptor::prelude::{AuthStatus, OAuthStart, ProviderDescriptor};

use crate::error::AppError;

/// One connectable provider, addressed by its stable [`ProviderDescriptor::id`].
///
/// Every method maps onto the FE-facing generic IPC: `auth_status` →
/// `*_provider_status`, `connect_fields` → `*_provider_connect_fields`,
/// `start_oauth` → `*_provider_start_oauth`, `disconnect` →
/// `*_provider_disconnect`.
#[async_trait]
pub trait ProviderConnector: Send + Sync {
    /// Stable id — matches `self.descriptor().id` and is the IPC routing key.
    fn id(&self) -> &'static str;

    /// Static metadata the FE renders the connect UI from. Forwarded verbatim
    /// from the underlying domain trait.
    fn descriptor(&self) -> ProviderDescriptor;

    /// Current authentication state, mapped onto the shared shape.
    async fn auth_status(&self) -> AuthStatus;

    /// Save credentials for a `Fields` auth method. `fields` keys match the
    /// descriptor's [`AuthField::key`](corvus_provider_descriptor::prelude::AuthField).
    /// Providers with no `Fields` method return `Err` — but the FE won't call
    /// it for them (the descriptor tells it which methods exist).
    async fn connect_fields(
        &self,
        method_id: &str,
        fields: HashMap<String, String>,
    ) -> Result<(), AppError>;

    /// Begin an OAuth method; the returned [`OAuthStart`] tells the FE how to
    /// proceed (open a URL, or show a device code). Completion is signalled
    /// out-of-band by the unified event `arbor://provider-oauth-done`
    /// `{ id, ok, error }`.
    async fn start_oauth(&self, method_id: &str, app: AppHandle) -> Result<OAuthStart, AppError>;

    /// Remove all stored credentials for this provider.
    async fn disconnect(&self) -> Result<(), AppError>;
}

/// A registry of [`ProviderConnector`]s keyed by id. Built once per domain
/// (see [`issue::issue_connectors`] / [`git::git_connectors`]) and consumed by
/// the (next-phase) generic Tauri commands.
#[derive(Default)]
pub struct ConnectorRegistry {
    connectors: HashMap<&'static str, Box<dyn ProviderConnector>>,
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self { connectors: HashMap::new() }
    }

    /// Register a connector under its own [`ProviderConnector::id`], replacing
    /// any prior one.
    pub fn register(&mut self, c: Box<dyn ProviderConnector>) {
        self.connectors.insert(c.id(), c);
    }

    /// The connector registered under `id`, if any.
    pub fn get(&self, id: &str) -> Option<&dyn ProviderConnector> {
        self.connectors.get(id).map(|b| b.as_ref())
    }

    /// The descriptors of every registered connector (for the FE provider list).
    pub fn descriptors(&self) -> Vec<ProviderDescriptor> {
        self.connectors.values().map(|c| c.descriptor()).collect()
    }
}

// ── AuthStatus mapping helpers ────────────────────────────────────────────────

/// Map the issue-tracker-domain [`AuthStatus`](corvus_issue_tracker_api::prelude::AuthStatus)
/// onto the shared FE-facing [`AuthStatus`].
///
/// `account_label` ← `domain` (Jira tenant host / `None` for single-tenant),
/// `method` ← `auth_method`, and the user shape is narrowed to the shared
/// [`ProviderUserInfo`](corvus_provider_descriptor::prelude::ProviderUserInfo).
pub fn map_issue_auth_status(s: corvus_issue_tracker_api::prelude::AuthStatus) -> AuthStatus {
    use corvus_provider_descriptor::prelude::ProviderUserInfo;
    AuthStatus {
        authenticated: s.authenticated,
        user: s.user.map(|u| ProviderUserInfo {
            display_name: u.display_name,
            email:        u.email,
            avatar_url:   u.avatar_url,
        }),
        account_label: s.domain,
        method:        s.auth_method,
    }
}

/// Compose a git-host [`AuthStatus`] from a provider's sync `has_token()` plus
/// (when authenticated) its async `current_user()`.
///
/// - `authenticated` ← `has_token()`.
/// - On auth, `current_user()` errors are swallowed → `user = None`.
/// - `ProviderUser` → `ProviderUserInfo` with `display_name = name.unwrap_or(login)`.
/// - `account_label` is the caller's responsibility (self-hosted host vs `None`
///   for github.com/gitlab.com) — passed in.
/// - `method` is best-effort — passed in (`Some("oauth")` when connected via
///   OAuth, `None` acceptable).
pub async fn compose_git_auth_status(
    provider: &dyn corvus_git_provider_api::prelude::GitProvider,
    account_label: Option<String>,
    method: Option<String>,
) -> AuthStatus {
    use corvus_provider_descriptor::prelude::ProviderUserInfo;

    let authenticated = provider.has_token();
    let user = if authenticated {
        match provider.current_user().await {
            Ok(u) => Some(ProviderUserInfo {
                display_name: u.name.unwrap_or(u.login),
                email:        u.email,
                avatar_url:   u.avatar_url,
            }),
            Err(_) => None,
        }
    } else {
        None
    };

    AuthStatus {
        authenticated,
        user,
        account_label,
        // Only surface a method once there's actually a token.
        method: if authenticated { method } else { None },
    }
}
