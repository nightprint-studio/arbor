//! `VaultSessionProvider` — the single, descriptor-driven shell-side
//! [`SessionProvider`] that replaces the four hand-written per-provider adapters
//! (GitHub, GitLab, Linear, Jira).
//!
//! All four adapters shared the *exact* same `session`/`refresh`/`has_credentials`
//! control flow — read a session, on a 401/403 run the provider's refresh then
//! re-read, probe presence — and differed only in three small functions: how the
//! token is read from the keyring (each with its own priority rules), how it is
//! refreshed (each with its own lock / guard), and the cheap presence probe.
//!
//! Those three functions, plus the provider's labels, are bundled into one
//! [`CredentialDescriptor`] value per provider. The shared control flow lives
//! once, here, in [`VaultSessionProvider`]; adding a provider is one descriptor.
//!
//! This is the launcher-side vault from `docs/credential-architecture.md`. The
//! genuinely-divergent bits (token read, OAuth refresh) stay as referenced
//! shell functions for now — they encapsulate priority order, the single-use
//! refresh lock, and Jira's multi-mode `get_config` branching, which must not be
//! re-implemented. Turning the descriptor into pure, serializable data (so each
//! backend can own + ship its own via the `__credential_descriptors` IPC, and an
//! OOP backend resolves over the reverse channel) is the next increment; this
//! step keeps behaviour identical and collapses the four impls into one.

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use arbor_ipc::prelude::{AuthSession, CredentialError, SessionProvider};

use crate::auth::{credential_store, oauth_jira, oauth_linear};
use crate::error::Result;
use crate::git_provider::ci_impl;
use crate::git_provider::oauth::{github_flow, gitlab_flow};

/// Linear's GraphQL endpoint — the `base_url` of a Linear session. Owned here
/// now that the launcher no longer depends on the `corvus-issue-tracker-*`
/// crates (the issue-tracker logic moved out of process into `corvus-be`); this
/// is the one constant the shell-side vault still needs to shape a session.
const LINEAR_GQL: &str = "https://api.linear.app/graphql";

/// Linear's single keyring slot — the access token, shared between the OAuth and
/// PAT flows (see `oauth_linear`).
const LINEAR_KR_HOST: &str = "linear.app";
const LINEAR_KR_USER: &str = "api-key";

/// A boxed, `Send` future for the refresh function pointers (async fns can't be
/// stored as plain `fn` pointers, so each provider's refresh is a thunk that
/// boxes its future).
type RefreshFut = Pin<Box<dyn Future<Output = Result<bool>> + Send>>;

/// One provider's credential resolution: its labels plus the three functions
/// that differ between providers. The shared session/refresh/has control flow
/// lives in [`VaultSessionProvider`].
pub struct CredentialDescriptor {
    /// Lowercase id used in `NotFound` errors (e.g. `"github"`).
    id: &'static str,
    /// Display name used in "nothing to refresh" messages (e.g. `"GitHub"`).
    display: &'static str,
    /// Read the current session: keyring lookup + `{ base_url, header, web }`
    /// shaping. `Ok(None)` = no credential stored; `Err` = store failure.
    read: fn(&str) -> Result<Option<AuthSession>>,
    /// Refresh after a 401/403. `Ok(false)` = nothing to refresh (PAT / static
    /// token / self-hosted) — the caller then surfaces the original auth error.
    refresh: fn(String) -> RefreshFut,
    /// Cheap, synchronous presence probe (mirrors the old `has_credentials`).
    has: fn(&str) -> bool,
}

// ── Read functions (keyring lookup + session shaping) ─────────────────────────

/// GitHub: fixed `api.github.com` `Bearer` session; `account` is informational.
fn read_github(_account: &str) -> Result<Option<AuthSession>> {
    Ok(ci_impl::get_github_token()?.map(|token| AuthSession {
        base_url:    "https://api.github.com".to_string(),
        auth_header: format!("Bearer {token}"),
        web_base:    Some("https://github.com".to_string()),
    }))
}

/// GitLab: the `account` IS the instance host root (e.g. `https://gitlab.com`
/// or a self-hosted URL); `get_gitlab_token` keys the store off it.
fn read_gitlab(account: &str) -> Result<Option<AuthSession>> {
    Ok(ci_impl::get_gitlab_token(account)?.map(|token| AuthSession {
        base_url:    account.to_string(),
        auth_header: format!("Bearer {token}"),
        web_base:    Some(account.to_string()),
    }))
}

/// Linear: fixed GraphQL endpoint `Bearer` session; `account` is informational.
fn read_linear(_account: &str) -> Result<Option<AuthSession>> {
    Ok(credential_store::get(LINEAR_KR_HOST, LINEAR_KR_USER)?.map(|token| AuthSession {
        base_url:    LINEAR_GQL.to_string(),
        auth_header: format!("Bearer {token}"),
        web_base:    None, // Linear's API returns issue URLs directly.
    }))
}

/// Jira: per-tenant base URL + `Bearer`/`Basic` header, resolved by
/// `oauth_jira::get_config` (which branches on the stored auth mode).
fn read_jira(_account: &str) -> Result<Option<AuthSession>> {
    Ok(oauth_jira::get_config()?.map(|cfg| AuthSession {
        base_url:    cfg.base_url,
        auth_header: cfg.auth_header,
        web_base:    cfg.domain,
    }))
}

// ── Refresh functions (boxed async thunks) ────────────────────────────────────

fn refresh_github(_account: String) -> RefreshFut {
    // GitHub "Expiring user tokens" use single-use refresh tokens; the flow
    // serializes + coalesces concurrent refreshes internally.
    Box::pin(async move { github_flow::try_refresh().await })
}

fn refresh_gitlab(account: String) -> RefreshFut {
    Box::pin(async move {
        // OAuth refresh exists only for gitlab.com; self-hosted uses PATs, which
        // have nothing to refresh (the 401 then propagates as before).
        if !account.contains("gitlab.com") {
            return Ok(false);
        }
        gitlab_flow::try_refresh().await
    })
}

fn refresh_linear(_account: String) -> RefreshFut {
    Box::pin(async move { oauth_linear::try_refresh().await })
}

fn refresh_jira(_account: String) -> RefreshFut {
    Box::pin(async move { oauth_jira::try_refresh().await })
}

// ── Presence probes ───────────────────────────────────────────────────────────

fn has_github(_account: &str) -> bool {
    ci_impl::get_github_token().ok().flatten().is_some()
}

fn has_gitlab(account: &str) -> bool {
    ci_impl::get_gitlab_token(account).ok().flatten().is_some()
}

/// Linear / Jira had no `has_credentials` override (the trait default is `true`).
fn has_always(_account: &str) -> bool {
    true
}

// ── Descriptors ───────────────────────────────────────────────────────────────

static GITHUB: CredentialDescriptor = CredentialDescriptor {
    id: "github", display: "GitHub",
    read: read_github, refresh: refresh_github, has: has_github,
};

static GITLAB: CredentialDescriptor = CredentialDescriptor {
    id: "gitlab", display: "GitLab",
    read: read_gitlab, refresh: refresh_gitlab, has: has_gitlab,
};

static LINEAR: CredentialDescriptor = CredentialDescriptor {
    id: "linear", display: "Linear",
    read: read_linear, refresh: refresh_linear, has: has_always,
};

static JIRA: CredentialDescriptor = CredentialDescriptor {
    id: "jira", display: "Jira",
    read: read_jira, refresh: refresh_jira, has: has_always,
};

// ── The one generic provider ──────────────────────────────────────────────────

/// The single, descriptor-driven shell-side credential provider. Construct one
/// per backend via the named constructors; each backend holds its own
/// `Arc<dyn SessionProvider>` exactly as before — only the impl is now shared.
pub struct VaultSessionProvider {
    desc: &'static CredentialDescriptor,
}

impl VaultSessionProvider {
    pub fn github() -> Self { Self { desc: &GITHUB } }
    pub fn gitlab() -> Self { Self { desc: &GITLAB } }
    pub fn linear() -> Self { Self { desc: &LINEAR } }
    pub fn jira()   -> Self { Self { desc: &JIRA } }

    /// Route an opaque `account` to the right provider — the inverse of how each
    /// in-process backend is constructed with its own provider. Used by the
    /// reverse channel's `__session`/`__refresh` host-handlers, which receive
    /// only the account string from an OOP backend. The fixed ids match what the
    /// backends pass (`"github.com"` / `"linear"` / `"jira"`); anything else is a
    /// GitLab instance host root (`https://gitlab.com` or self-hosted).
    pub fn for_account(account: &str) -> Self {
        match account {
            "github.com" => Self::github(),
            "linear"     => Self::linear(),
            "jira"       => Self::jira(),
            _            => Self::gitlab(),
        }
    }
}

#[async_trait]
impl SessionProvider for VaultSessionProvider {
    async fn session(&self, account: &str) -> std::result::Result<AuthSession, CredentialError> {
        match (self.desc.read)(account) {
            Ok(Some(s)) => Ok(s),
            Ok(None)    => Err(CredentialError::NotFound(self.desc.id.into())),
            Err(e)      => Err(CredentialError::Store(e.to_string())),
        }
    }

    async fn refresh(&self, account: &str) -> std::result::Result<AuthSession, CredentialError> {
        let refreshed = (self.desc.refresh)(account.to_string())
            .await
            .map_err(|e| CredentialError::Refresh(e.to_string()))?;
        if !refreshed {
            // PAT / non-expiring / self-hosted: nothing to refresh — the caller
            // surfaces the original 401 as the usual auth error.
            return Err(CredentialError::Refresh(format!("no {} refresh token", self.desc.display)));
        }
        match (self.desc.read)(account) {
            Ok(Some(s)) => Ok(s),
            Ok(None)    => Err(CredentialError::Refresh(format!("no credential stored for '{}'", self.desc.id))),
            Err(e)      => Err(CredentialError::Refresh(e.to_string())),
        }
    }

    fn has_credentials(&self, account: &str) -> bool {
        (self.desc.has)(account)
    }
}
