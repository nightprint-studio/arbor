//! Resolution helpers used by Tauri command layers.
//!
//! Phase 4 routes every consumer through `provider_for_tab` so commands
//! never touch `crate::git_provider::mr_impl::*` / `crate::git_provider::ci_impl::*` directly.
//! The function returns the registered `Arc<dyn GitProvider>` plus a
//! `RepoRef` shaped for that provider, alongside the legacy
//! `CiProviderInfo` that callers still need for owner / project_path /
//! base_url string fields when invoking trait gaps.

use std::sync::Arc;

use crate::AppState;
use crate::error::{AppError, Result};
use crate::git_provider::ci_impl::CiProviderInfo;

use super::{GitProvider, GitlabProvider, types::RepoRef};

/// Tuple returned by `provider_for_tab` — caller chooses which fields it
/// needs.  `repo_ref` is shaped for the provider's REST conventions
/// (GitHub: owner+name; GitLab: full project path).
pub struct ResolvedProvider {
    pub provider: Arc<dyn GitProvider>,
    pub repo:     RepoRef,
    pub info:     CiProviderInfo,
}

/// Resolve `(provider, repo_ref, info)` for the repo currently bound to
/// `tab_id`.  Errors when the tab has no GitHub/GitLab remote, or when
/// no provider is registered for the matched host.
///
/// For self-hosted GitLab instances that aren't pre-registered, this
/// auto-registers a `GitlabProvider::new_self_hosted(host)` on demand so
/// downstream callers always get an `Arc<dyn GitProvider>`.
pub fn provider_for_tab(state: &AppState, tab_id: &str) -> Result<ResolvedProvider> {
    let remotes: Vec<(String, String)> = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(tab_id)?;
        crate::git::remote::list_remotes(repo.inner())?
            .into_iter()
            .map(|r| (r.name, r.url))
            .collect()
    };
    provider_for_remotes(state, &remotes)
}

/// Variant of `provider_for_tab` for callers that work off a repo path
/// rather than a tab id (e.g. plugins iterating the registry).  Opens the
/// repo via libgit2 directly — no tab manager involvement.
pub fn provider_for_path(state: &AppState, path: &str) -> Result<ResolvedProvider> {
    let repo = git2::Repository::open(path)
        .map_err(|e| AppError::Other(format!("open '{path}': {e}")))?;
    let remotes: Vec<(String, String)> = crate::git::remote::list_remotes(&repo)?
        .into_iter()
        .map(|r| (r.name, r.url))
        .collect();
    provider_for_remotes(state, &remotes)
}

/// Shared resolution body: given a list of `(name, url)` remotes, detect
/// the provider, look up (or auto-register) the trait impl, and build the
/// matching `RepoRef`.
fn provider_for_remotes(
    state:   &AppState,
    remotes: &[(String, String)],
) -> Result<ResolvedProvider> {
    let info = crate::git_provider::ci_impl::detect_from_remotes(remotes)
        .ok_or_else(|| AppError::Other(
            "No GitHub or GitLab remote detected for this repository".into(),
        ))?;

    let provider = lookup_or_register(state, &info)?;

    let repo_ref = match info.provider.as_str() {
        "github" => RepoRef::github(
            info.owner.clone().unwrap_or_default(),
            info.repo_name.clone().unwrap_or_default(),
        ),
        "gitlab" => RepoRef::gitlab(info.project_path.clone().unwrap_or_default()),
        other => return Err(AppError::Other(format!("Unknown provider: {other}"))),
    };

    Ok(ResolvedProvider { provider, repo: repo_ref, info })
}

fn lookup_or_register(
    state: &AppState,
    info:  &CiProviderInfo,
) -> Result<Arc<dyn GitProvider>> {
    // Try direct lookup first.
    {
        let registry = state.lock_git_providers()?;
        if let Some(p) = registry.for_remote_url(&info.remote_url) {
            return Ok(p);
        }
    }

    // For GitLab self-hosted instances we can construct a provider on
    // demand so callers never see "no provider for host X" errors.
    if info.provider == "gitlab" {
        if let Some(base_url) = info.gitlab_base_url.as_deref() {
            let mut registry = state.lock_git_providers()?;
            // Check again under the write lock — another thread may have
            // raced ahead and registered it.
            if let Some(p) = registry.for_remote_url(&info.remote_url) {
                return Ok(p);
            }
            let provider = Arc::new(GitlabProvider::new_self_hosted(base_url));
            registry.register(provider.clone());
            return Ok(provider);
        }
    }

    Err(AppError::Other(format!(
        "No GitProvider registered for remote {}", info.remote_url
    )))
}

/// Build a `MrId` from a `ResolvedProvider` + numeric MR/PR id.  Branches
/// on `info.provider` so GitHub gets `(owner, repo_name)` and GitLab gets
/// `(project_path, None)`.
pub fn mr_id_from(resolved: &ResolvedProvider, number: u64) -> super::types::MrId {
    use super::ProviderKind;
    use super::types::MrId;
    match resolved.info.provider.as_str() {
        "github" => MrId {
            provider:      ProviderKind::GitHub,
            owner_or_path: resolved.info.owner.clone().unwrap_or_default(),
            repo_name:     Some(resolved.info.repo_name.clone().unwrap_or_default()),
            number,
        },
        _ => MrId {
            provider:      ProviderKind::GitLab,
            owner_or_path: resolved.info.project_path.clone().unwrap_or_default(),
            repo_name:     None,
            number,
        },
    }
}
