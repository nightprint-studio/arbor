//! Canonical entry point for `corvus-git-provider-github`'s public API.
//!
//! Call sites import `use corvus_git_provider_github::prelude::*;` (or the
//! fully-qualified `corvus_git_provider_github::prelude::GithubProvider`).

pub use crate::http::GithubHttp;
pub use crate::provider::GithubProvider;
