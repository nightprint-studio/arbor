//! GitHub releases — STUB. Every method returns `Unsupported`.

use corvus_git_provider_api::prelude::*;

use crate::http::GithubHttp;

pub(crate) async fn list_releases(
    _http: &GithubHttp,
    _repo: &RepoRef,
) -> Result<Vec<Release>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_releases".into() })
}

pub(crate) async fn get_release(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _id:   &str,
) -> Result<Release, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_release".into() })
}

pub(crate) async fn create_release(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _req:  ReleaseCreateRequest,
) -> Result<Release, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_release".into() })
}

pub(crate) async fn delete_release(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _id:   &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "delete_release".into() })
}
