//! `corvus-git-provider-api` — the provider-agnostic git-host contract.
//!
//! This crate holds the *vocabulary* of the git-provider domain: the DTOs that
//! cross the IPC boundary (merge requests, CI runs, releases, repo-native
//! issues, webhooks, security findings, …), the async object-safe
//! [`GitProvider`](prelude::GitProvider) trait, and a host-keyed trait-object
//! [`GitProviderRegistry`](prelude::GitProviderRegistry).
//!
//! It is the leaf the concrete impl crates (`corvus-git-provider-github`,
//! `corvus-git-provider-gitlab`) build on. It pulls in **no** client logic and
//! **no** credential store — only `serde`, `async-trait`, `thiserror`, and the
//! HTTP/JSON error-conversion surface needed by `ProviderError`.
//!
//! Per the workspace convention, import via the [`prelude`]:
//! `use corvus_git_provider_api::prelude::*;`.

pub mod auth;
pub mod avatar;
pub mod branch;
pub mod capability;
pub mod ci;
pub mod error;
pub mod issue;
pub mod kind;
pub mod mr;
pub mod provider;
pub mod registry;
pub mod release;
pub mod repo;
pub mod security;
pub mod security_export;
pub mod webhook;

pub mod prelude;

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    #[test]
    fn provider_kind_is_lowercase() {
        assert_eq!(serde_json::to_string(&ProviderKind::GitHub).unwrap(), "\"github\"");
        assert_eq!(serde_json::to_string(&ProviderKind::GitLab).unwrap(), "\"gitlab\"");
    }

    #[test]
    fn mr_state_is_lowercase() {
        assert_eq!(serde_json::to_string(&MrState::Open).unwrap(), "\"open\"");
        assert_eq!(serde_json::to_string(&MrState::Merged).unwrap(), "\"merged\"");
    }

    #[test]
    fn provider_error_is_tagged() {
        let e = ProviderError::Http { status: 404, body: "nope".into() };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(v["kind"], "Http");
        assert_eq!(v["data"]["status"], 404);
    }

    #[test]
    fn severity_round_trips_from_gitlab() {
        assert_eq!(Severity::from_gitlab("CRITICAL"), Severity::Critical);
        assert_eq!(Severity::from_gitlab("bogus"), Severity::Unknown);
        assert_eq!(Severity::Critical.gitlab_enum(), "CRITICAL");
    }
}
