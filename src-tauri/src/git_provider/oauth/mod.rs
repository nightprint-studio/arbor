//! Provider-side OAuth flow integrations.
//!
//! `github` / `gitlab` are the trait-facing wrappers (`start`/`revoke_token`)
//! used by `GithubProvider` / `GitlabProvider`.  `github_flow` / `gitlab_flow`
//! hold the actual OAuth implementations (Authorization Code + PKCE,
//! local-loopback callback listener) — relocated from `crate::auth::oauth_*`
//! in Phase 5.

pub mod github;
pub mod gitlab;
pub mod github_flow;
pub mod gitlab_flow;
