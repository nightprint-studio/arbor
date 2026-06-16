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

use async_trait::async_trait;
use thiserror::Error;

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
#[derive(Debug, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;
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
}
