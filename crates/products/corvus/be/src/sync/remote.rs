//! Resolve (or create) the private repo the bundle lives in.
//!
//! The provider is the one already wired for the OOP REST domains
//! ([`crate::provider`], reverse-channel credentials). On enable we ask the
//! provider for the authenticated user, pick a repo name (explicit or the
//! default), then probe: an existing repo is **adopted**, a 404 means we
//! **create it private**. Auto-created + reused-across-machines share the same
//! default name, so leaving the name blank on a second machine adopts the repo
//! the first one made.

use corvus_git_provider_api::prelude::{
    ProviderError, RemoteRepoInfo, RepoCreateRequest, RepoRef, RepoVisibility,
};

/// The default repo name when the user doesn't specify one. Fixed (not suffixed)
/// so a second machine adopts the same repo instead of forking a new one.
pub(crate) const DEFAULT_REPO_NAME: &str = "arbor-corvus-sync";

/// A resolved sync target: everything the engine needs to read/write the bundle.
#[derive(Debug, Clone)]
pub(crate) struct SyncRemote {
    pub provider_key: String,
    pub repo_ref: RepoRef,
    pub full_name: String,
    pub clone_url: String,
    pub branch: String,
}

/// Rebuild a [`SyncRemote`] from persisted config (`provider` + `owner/name`),
/// without a network round-trip — used by the driver and manual push. `None`
/// when the config isn't fully resolved yet.
pub(crate) fn from_config(cfg: &crate::corvus_config::SyncConfig) -> Option<SyncRemote> {
    let provider_key = cfg.provider.clone()?;
    let full_name = cfg.repo_full_name.clone()?;
    let (owner, name) = full_name.split_once('/')?;
    Some(SyncRemote {
        provider_key,
        repo_ref: RepoRef::github(owner.to_string(), name.to_string()),
        full_name: full_name.clone(),
        clone_url: cfg.clone_url.clone().unwrap_or_default(),
        branch: super::BRANCH.to_string(),
    })
}

/// Resolve the sync repo for `provider_key`, creating it private if absent.
pub(crate) async fn resolve_or_create(
    provider_key: &str,
    repo_name: Option<&str>,
) -> Result<SyncRemote, String> {
    let provider = crate::provider::for_host(provider_key)?;
    let user = provider.current_user().await.map_err(crate::provider::pe)?;
    let owner = user.login;
    let name = match repo_name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => DEFAULT_REPO_NAME.to_string(),
    };

    match provider.get_repo(&owner, &name).await {
        Ok(info) => Ok(from_info(provider_key, &owner, &name, &info)),
        Err(e) if is_absent(&e) => {
            let req = RepoCreateRequest {
                name: name.clone(),
                description: Some("Arbor corvus settings sync (private).".to_string()),
                visibility: RepoVisibility::Private,
                org: None,
                namespace_id: None,
            };
            let info = provider.create_repo(req).await.map_err(crate::provider::pe)?;
            Ok(from_info(provider_key, &owner, &name, &info))
        }
        Err(e) => Err(crate::provider::pe(e)),
    }
}

fn from_info(provider_key: &str, owner: &str, name: &str, info: &RemoteRepoInfo) -> SyncRemote {
    let branch = if info.default_branch.trim().is_empty() {
        super::BRANCH.to_string()
    } else {
        info.default_branch.clone()
    };
    SyncRemote {
        provider_key: provider_key.to_string(),
        repo_ref: RepoRef::github(owner.to_string(), name.to_string()),
        full_name: info.full_name.clone(),
        clone_url: info.clone_url_https.clone(),
        branch,
    }
}

/// Whether a provider error means "the repo doesn't exist" (→ create it).
fn is_absent(e: &ProviderError) -> bool {
    matches!(e, ProviderError::Http { status: 404, .. } | ProviderError::NotFound(_))
}
