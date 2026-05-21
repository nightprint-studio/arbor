//! GitLab security dashboard — thin wrappers over `security_impl`.
//!
//! The trait dispatcher in `gitlab/mod.rs` calls into here; the heavy
//! lifting (GraphQL queries, pagination, finding parser) lives in
//! `crate::git_provider::security_impl`. This file is purely the
//! `RepoRef + base_url + token` → impl-function plumbing.

use crate::git_provider::types::{
    RepoRef, SecuritySummary, SecurityFinding, SecurityFilters,
    error::ProviderError,
};

use super::api;

fn token(base_url: &str) -> Result<String, ProviderError> {
    api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)
}

fn project_path<'a>(repo: &'a RepoRef) -> &'a str {
    repo.owner_or_path.as_str()
}

pub async fn supports_security(
    base_url: &str,
    repo:     &RepoRef,
) -> Result<bool, ProviderError> {
    let path = project_path(repo);
    // Without a token we can't probe; treat it as "not supported" so the
    // gating UI hides the entry rather than rendering a misleading "no
    // findings" state.
    let Some(tok) = api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(e.to_string()))?
    else {
        return Ok(false);
    };
    crate::git_provider::security_impl::gitlab_supports_security(path, base_url, &tok)
        .await
        .map_err(ProviderError::from)
}

pub async fn fetch_security_summary(
    base_url:   &str,
    repo:       &RepoRef,
    range_days: u32,
) -> Result<SecuritySummary, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    crate::git_provider::security_impl::fetch_gitlab_security_summary(
        path, base_url, &token, range_days,
    )
    .await
    .map_err(ProviderError::from)
}

pub async fn fetch_security_findings(
    base_url: &str,
    repo:     &RepoRef,
    filters:  SecurityFilters,
) -> Result<Vec<SecurityFinding>, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    let (findings, _truncated) = crate::git_provider::security_impl::fetch_gitlab_security_findings(
        path, base_url, &token, &filters,
    )
    .await
    .map_err(ProviderError::from)?;

    // Server-side filtering covers severity/state/report_type; the `search`
    // clause is host-side. Apply it here so callers always get a
    // post-filter list regardless of the entry point.
    Ok(crate::git_provider::security_impl::apply_filters(findings, &filters))
}
