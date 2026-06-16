//! Canonical entry point for `corvus-git-provider-gitlab`'s public API.
//!
//! Call sites import `use corvus_git_provider_gitlab::prelude::*;` (or the
//! fully-qualified `corvus_git_provider_gitlab::prelude::GitlabProvider`).

pub use crate::http::GitlabHttp;
pub use crate::provider::GitlabProvider;
