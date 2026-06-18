//! [`SessionProvider`] — the keyring-free credential contract an HTTP API
//! backend depends on (Model D, §D.5).
//!
//! A headless backend (`corvus-be`, …) never touches the OS keyring and never
//! runs the OAuth dance itself: it asks for an [`AuthSession`] (the base URL +
//! the full `Authorization` header to use), and — on a `401`/`403` — asks for a
//! refreshed one before retrying. The shell side is the *sole* keyring holder;
//! how a stale credential is renewed (re-reading a rotated secret, or running an
//! OAuth refresh) is its business, opaque to the backend.
//!
//! Why a *session* and not just a token: providers vary. Linear is a fixed
//! endpoint + `Bearer`; Jira is a per-tenant base URL + either `Bearer` (OAuth)
//! or `Basic` (API token); self-hosted GitLab/GitHub bring their own base URL.
//! Carrying `{ base_url, auth_header }` covers them all with one abstraction.
//!
//! The trait is **async**: resolving a credential is I/O (keyring read, OAuth
//! HTTP refresh) and every call site is already `async`. The same contract works
//! in-process (a shell-side impl awaits a local refresh) and, later, over IPC
//! (the impl awaits a round-trip to the shell). It stays free of `keyring`/HTTP
//! types so the coupled domains — issue trackers, git providers — can be
//! extracted into `corvus-*` crates that hold an `Arc<dyn SessionProvider>`.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::host::HostCaller;

/// A credential-resolution failure, kept free of `keyring`/transport types so a
/// backend crate can surface it without linking the broker.
#[derive(Debug, Error)]
pub enum CredentialError {
    /// No credential is stored for this account.
    #[error("no credential stored for '{0}'")]
    NotFound(String),
    /// The credential store (keyring on the shell side) failed.
    #[error("credential store: {0}")]
    Store(String),
    /// A refresh was attempted but failed (no refresh secret, provider rejected
    /// it, nothing to refresh for a static credential, …).
    #[error("refresh failed: {0}")]
    Refresh(String),
}

pub type Result<T> = std::result::Result<T, CredentialError>;

/// What a backend needs to make an authenticated request: the base URL to hit
/// and the full `Authorization` header value to send (`"Bearer …"`, `"Basic …"`).
///
/// `Serialize`/`Deserialize` so it can cross the reverse channel: the shell's
/// `VaultSessionProvider` resolves it and marshals it back to a
/// [`ChildSessionProvider`] in the backend process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    /// API base URL (per-tenant for some providers, fixed for others).
    pub base_url: String,
    /// The complete `Authorization` header value.
    pub auth_header: String,
    /// User-facing web host for building links (e.g. Jira's `you.atlassian.net`,
    /// or a git host vs its API host). `None` when the API itself returns links
    /// (Linear) — the backend then doesn't construct them.
    pub web_base: Option<String>,
}

/// The credential contract a backend depends on: hand me a session for
/// `account`, and — when the provider rejects it — hand me a refreshed one.
///
/// `account` is an opaque path the backend passes through (e.g. `"linear"`); the
/// shell-side impl maps it to the real keyring entry, so only the shell can
/// reach the keyring.
#[async_trait]
pub trait SessionProvider: Send + Sync {
    /// The current session for `account` (from cache when fresh, otherwise
    /// resolved by the shell).
    async fn session(&self, account: &str) -> Result<AuthSession>;

    /// Obtain a fresh session after a `401`/`403`. The shell drops the stale
    /// credential, runs the provider's refresh, and returns the new session.
    /// Errors when there's nothing to refresh (e.g. a static API token).
    async fn refresh(&self, account: &str) -> Result<AuthSession>;

    /// Cheap, synchronous presence probe: does a credential exist for `account`?
    ///
    /// Backends whose trait surface has a sync "is connected" affordance (e.g.
    /// `GitProvider::has_token`) need this without paying for an async keyring
    /// read or an HTTP round-trip. The default is `true` — providers that only
    /// resolve lazily (and whose callers never ask) need not implement it; the
    /// shell adapters that gate UI on token presence override it with a real
    /// keyring check.
    fn has_credentials(&self, account: &str) -> bool {
        let _ = account;
        true
    }
}

/// The backend-side [`SessionProvider`] for an OOP process: it marshals
/// `session`/`refresh` over the reverse channel ([`HostCaller`]) to the shell's
/// `VaultSessionProvider` and awaits the resolved [`AuthSession`].
///
/// The backend holds an `Arc<dyn SessionProvider>` and **cannot tell** whether
/// it's this (OOP) or `VaultSessionProvider` (in-process) — the call site never
/// changes. `account` is the opaque provider key (`"linear"`, `"github.com"`, a
/// GitLab instance URL); the shell maps it to the real keyring entry.
pub struct ChildSessionProvider {
    host: Arc<dyn HostCaller>,
}

impl ChildSessionProvider {
    pub fn new(host: Arc<dyn HostCaller>) -> Self {
        Self { host }
    }

    /// Shared marshalling for `session`/`refresh`: call the shell host-method,
    /// then deserialize the `AuthSession` it returns.
    fn resolve(&self, method: &str, account: &str, wrap: fn(String) -> CredentialError) -> Result<AuthSession> {
        // `HostCaller::call` is synchronous (it blocks on the shell's reply); the
        // `async` trait method does no further awaiting, so a runtime-less backend
        // can drive it. The reply arrives via the serve loop's reader thread, so
        // the block doesn't deadlock the call.
        let value = self.host.call(method, serde_json::json!(account)).map_err(wrap)?;
        serde_json::from_value(value).map_err(|e| wrap(e.to_string()))
    }
}

#[async_trait]
impl SessionProvider for ChildSessionProvider {
    async fn session(&self, account: &str) -> Result<AuthSession> {
        self.resolve("__session", account, CredentialError::Store)
    }

    async fn refresh(&self, account: &str) -> Result<AuthSession> {
        self.resolve("__refresh", account, CredentialError::Refresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;
    use serde_json::Value;
    use std::sync::Mutex;

    /// A trivial in-memory `SessionProvider`: `refresh` flips the account to a
    /// rotated header, modelling an on-401 refresh.
    struct FakeProvider {
        rotated: Mutex<bool>,
    }

    #[async_trait]
    impl SessionProvider for FakeProvider {
        async fn session(&self, account: &str) -> Result<AuthSession> {
            if account == "absent" {
                return Err(CredentialError::NotFound(account.to_string()));
            }
            let rotated = *self.rotated.lock().unwrap();
            Ok(AuthSession {
                base_url: "https://api.example.com".into(),
                auth_header: format!("Bearer {account}:{}", if rotated { "v2" } else { "v1" }),
                web_base: None,
            })
        }

        async fn refresh(&self, account: &str) -> Result<AuthSession> {
            *self.rotated.lock().unwrap() = true;
            self.session(account).await
        }
    }

    #[test]
    fn session_then_refresh_yields_rotated_header() {
        let p = FakeProvider { rotated: Mutex::new(false) };
        assert_eq!(block_on(p.session("github")).unwrap().auth_header, "Bearer github:v1");
        assert_eq!(block_on(p.refresh("github")).unwrap().auth_header, "Bearer github:v2");
        assert_eq!(block_on(p.session("github")).unwrap().auth_header, "Bearer github:v2");
    }

    #[test]
    fn missing_account_is_not_found() {
        let p = FakeProvider { rotated: Mutex::new(false) };
        assert!(matches!(block_on(p.session("absent")), Err(CredentialError::NotFound(_))));
    }

    /// A `HostCaller` that answers `__session` with a per-account `AuthSession`,
    /// standing in for the shell's `VaultSessionProvider` over the reverse channel.
    struct FakeHost;
    impl HostCaller for FakeHost {
        fn call(&self, method: &str, params: Value) -> std::result::Result<Value, String> {
            let account: String = serde_json::from_value(params).unwrap();
            match method {
                "__session" => Ok(serde_json::to_value(AuthSession {
                    base_url: format!("https://api/{account}"),
                    auth_header: "Bearer tok".into(),
                    web_base: None,
                })
                .unwrap()),
                "__refresh" => Err("nothing to refresh".into()),
                other => Err(format!("unexpected host method: {other}")),
            }
        }
    }

    #[test]
    fn child_session_marshals_over_the_host_caller() {
        let p = ChildSessionProvider::new(Arc::new(FakeHost));
        let s = block_on(p.session("linear")).unwrap();
        assert_eq!(s.base_url, "https://api/linear");
        assert_eq!(s.auth_header, "Bearer tok");
        // A shell-side error crosses as a wire string wrapped in the right variant.
        assert!(matches!(block_on(p.refresh("linear")), Err(CredentialError::Refresh(_))));
    }
}
