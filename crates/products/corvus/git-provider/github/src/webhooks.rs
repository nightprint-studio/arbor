//! GitHub webhooks — STUB. Every method returns `Unsupported`.

use corvus_git_provider_api::prelude::*;

use crate::http::GithubHttp;

pub(crate) async fn list_webhooks(
    _http: &GithubHttp,
    _repo: &RepoRef,
) -> Result<Vec<Webhook>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_webhooks".into() })
}

pub(crate) async fn create_webhook(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _req:  WebhookCreateRequest,
) -> Result<Webhook, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_webhook".into() })
}

pub(crate) async fn delete_webhook(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _id:   &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "delete_webhook".into() })
}
