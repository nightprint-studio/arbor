//! GitLab remote branches via REST — STUB. Every method returns `Unsupported`.
//!
//! Mirrors the inline `Unsupported` arms in the old `git_provider::gitlab::mod`
//! (`list_remote_branches` / `get_default_branch` / `protect_branch`).

use corvus_git_provider_api::prelude::*;

use crate::http::GitlabHttp;

pub(crate) async fn list_remote_branches(
    _http: &GitlabHttp,
    _repo: &RepoRef,
) -> Result<Vec<String>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_remote_branches".into() })
}

pub(crate) async fn get_default_branch(
    _http: &GitlabHttp,
    _repo: &RepoRef,
) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_default_branch".into() })
}

pub(crate) async fn protect_branch(
    _http:   &GitlabHttp,
    _repo:   &RepoRef,
    _branch: &str,
    _req:    BranchProtection,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "protect_branch".into() })
}
