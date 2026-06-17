//! Shell-side git-provider integration.
//!
//! The provider *contract* — the `GitProvider` trait, `ProviderKind`, the
//! registry, and every DTO — lives in the `corvus-git-provider-api` crate and
//! is re-exported here so existing `crate::git_provider::*` call sites keep
//! resolving. This module owns only the shell glue: the host-keyed
//! registration (via the keyring-free `corvus-git-provider-{github,gitlab}`
//! crates + `SessionProvider`), the keyring/OAuth-coupled token lookups, and
//! the AppState resolution helpers. All MR/CI/security/repo-browser REST now
//! flows through the `GitProvider` trait; the only shell-side REST left is the
//! host-gated inline-image proxy (`image_proxy`, host-dynamic by design) plus
//! provider detection + token + 401-refresh senders (`ci_impl`, used by
//! `avatar_lookup` + `image_proxy`).

pub use corvus_git_provider_api::prelude::{GitProvider, GitProviderRegistry, ProviderKind};

pub mod types;
pub mod detect;
pub mod oauth;
pub mod session;
pub mod helpers;

// ── Shell-side helper modules ────────────────────────────────────────────────
// `mr_impl` / `repo_impl` are now just DTO aliases (`pub use ...api::{mr,repo}::*`)
// for legacy `git_provider::{mr_impl,repo_impl}::*` paths. `ci_impl` keeps
// provider detection + token retrieval + the 401-refresh senders used by
// `avatar_lookup` + `image_proxy`. `image_proxy` is the host-gated inline-image
// fetch (host-dynamic → intentionally not a trait method). All MR/CI/repo-browser
// REST now flows through the `GitProvider` trait.
pub mod mr_impl;
pub mod ci_impl;
pub mod repo_impl;
pub mod image_proxy;
pub mod security_export;
pub mod avatar_lookup;

// GitHub + GitLab are now the keyring-free `corvus-git-provider-{github,gitlab}`
// crates; the shell injects credentials via `session::{Github,Gitlab}SessionProvider`.
pub use corvus_git_provider_github::prelude::GithubProvider;
pub use corvus_git_provider_gitlab::prelude::GitlabProvider;
pub use helpers::{provider_for_tab, provider_for_path, mr_id_from};
