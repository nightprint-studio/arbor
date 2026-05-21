//! GitHub releases — STUB. Every method returns `Unsupported`.

use crate::git_provider::types::{
    Release, ReleaseCreateRequest, RepoRef, error::ProviderError,
};

pub async fn list_releases(_repo: &RepoRef) -> Result<Vec<Release>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_releases".into() })
}

pub async fn get_release(_repo: &RepoRef, _id: &str) -> Result<Release, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_release".into() })
}

pub async fn create_release(
    _repo: &RepoRef,
    _req:  ReleaseCreateRequest,
) -> Result<Release, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_release".into() })
}

pub async fn delete_release(_repo: &RepoRef, _id: &str) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "delete_release".into() })
}
