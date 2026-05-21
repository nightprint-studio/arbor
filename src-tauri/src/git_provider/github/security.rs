//! GitHub security dashboard — thin wrappers over `security_impl`.
//!
//! GitHub doesn't expose a single security endpoint; the heavy lifting
//! (parallel fetches across code-scanning, secret-scanning, and Dependabot
//! plus the host-side aggregation) lives in
//! `crate::git_provider::security_impl`. This module is purely the
//! `RepoRef + token` → impl-function plumbing.

use crate::git_provider::types::{
    RepoRef, SecuritySummary, SecurityFinding, SecurityFilters,
    error::ProviderError,
};

use super::api;

fn token() -> Result<String, ProviderError> {
    api::get_token()
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)
}

fn repo_parts<'a>(repo: &'a RepoRef) -> Result<(&'a str, &'a str), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name  = repo.name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub RepoRef requires name".into())
    })?;
    Ok((owner, name))
}

pub async fn supports_security(repo: &RepoRef) -> Result<bool, ProviderError> {
    // No token → no probe; treat as unsupported so the gating UI hides the
    // entry rather than rendering a misleading state.
    let Some(tok) = api::get_token()
        .map_err(|e| ProviderError::Internal(e.to_string()))?
    else {
        return Ok(false);
    };
    let (owner, name) = repo_parts(repo)?;
    crate::git_provider::security_impl::github_supports_security(owner, name, &tok)
        .await
        .map_err(ProviderError::from)
}

pub async fn fetch_security_summary(
    repo:       &RepoRef,
    range_days: u32,
) -> Result<SecuritySummary, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    crate::git_provider::security_impl::fetch_github_security_summary(
        owner, name, &token, range_days,
    )
    .await
    .map_err(ProviderError::from)
}

pub async fn fetch_security_findings(
    repo:    &RepoRef,
    filters: SecurityFilters,
) -> Result<Vec<SecurityFinding>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    let (findings, _truncated) =
        crate::git_provider::security_impl::fetch_github_security_findings(
            owner, name, &token, &filters,
        )
        .await
        .map_err(ProviderError::from)?;

    // GitHub doesn't support the `search` clause server-side; apply the
    // full filter set host-side so callers always get a post-filter list.
    Ok(crate::git_provider::security_impl::apply_filters(findings, &filters))
}
