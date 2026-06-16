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

use crate::error::AppError;

pub use corvus_git_provider_api::prelude::{
    GitProvider, GitProviderRegistry, ProviderError, ProviderKind,
};

pub mod types;
pub mod detect;
pub mod oauth;
pub mod session;
pub mod gitlab;
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

// GitHub is now the keyring-free `corvus-git-provider-github` crate; the shell
// injects credentials via `session::GithubSessionProvider`. GitLab still lives
// in-tree until its own extraction (Chunk 3).
pub use corvus_git_provider_github::prelude::GithubProvider;
pub use gitlab::GitlabProvider;
pub use helpers::{provider_for_tab, provider_for_path, mr_id_from};

/// Map a shell `AppError` into the provider-contract `ProviderError`.
///
/// Replaces the old `impl From<AppError> for ProviderError` — that conversion
/// can no longer be a `From` impl now that `ProviderError` lives in a separate
/// crate (the orphan rule forbids it in either crate). The delegate modules in
/// `github/` and `gitlab/` apply it via `.map_err(app_err_to_provider)`.
///
/// Recognises the canonical "GitHub/GitLab API 404" shape produced by
/// `mr_impl`/`ci_impl` so the trait layer surfaces a typed `NotFound` instead
/// of swallowing it into `Internal`. This drives the sidebar EmptyState (MR
/// feature unavailable) and lets the frontend degrade gracefully without
/// parsing error strings.
pub fn app_err_to_provider(err: AppError) -> ProviderError {
    let s = err.to_string();
    if s.contains("API 404") {
        return ProviderError::NotFound(s);
    }
    ProviderError::Internal(s)
}
