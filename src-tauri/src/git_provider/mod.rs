//! Shell-side git-provider integration.
//!
//! The provider *contract* — the `GitProvider` trait, `ProviderKind`, the
//! registry, and every DTO — lives in the `corvus-git-provider-api` crate and
//! is re-exported here so existing `crate::git_provider::*` call sites keep
//! resolving. This module owns only the shell glue: the concrete
//! `GithubProvider` / `GitlabProvider` impls, the host-keyed registration, the
//! REST client modules (`mr_impl`, `ci_impl`, `repo_impl`, `security_impl`),
//! the keyring/OAuth-coupled token lookups, and the AppState resolution helpers.
//!
//! Adding a new provider is a matter of creating a `git_provider/<name>/`
//! module with `struct <Name>Provider` and `impl GitProvider for ...`.

pub use corvus_git_provider_api::prelude::{
    GitProvider, GitProviderRegistry, ProviderError, ProviderKind,
};

pub mod types;
pub mod detect;
pub mod oauth;
pub mod session;
pub mod helpers;

// ── Implementation modules ───────────────────────────────────────────────────
// REST client code + provider-specific helpers that the trait impls in
// `github/` and `gitlab/` delegate to. The DTO struct definitions they used to
// own now live in `corvus-git-provider-api`; these modules re-export them so
// the legacy `git_provider::mr_impl::MergeRequest` paths still resolve.
pub mod mr_impl;
pub mod ci_impl;
pub mod repo_impl;
pub mod security_impl;
pub mod security_export;
pub mod avatar_lookup;

// GitHub + GitLab are now the keyring-free `corvus-git-provider-{github,gitlab}`
// crates; the shell injects credentials via `session::{Github,Gitlab}SessionProvider`.
pub use corvus_git_provider_github::prelude::GithubProvider;
pub use corvus_git_provider_gitlab::prelude::GitlabProvider;
pub use helpers::{provider_for_tab, provider_for_path, mr_id_from};
