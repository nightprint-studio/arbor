//! GitLab webhooks — STUB. Every method returns `Unsupported`.

use corvus_git_provider_api::prelude::*;

use crate::http::GitlabHttp;

pub(crate) async fn list_webhooks(
    _http: &GitlabHttp,
    _repo: &RepoRef,
) -> Result<Vec<Webhook>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_webhooks".into() })
}

pub(crate) async fn create_webhook(
    _http: &GitlabHttp,
    _repo: &RepoRef,
    _req:  WebhookCreateRequest,
) -> Result<Webhook, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_webhook".into() })
}

pub(crate) async fn delete_webhook(
    _http: &GitlabHttp,
    _repo: &RepoRef,
    _id:   &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "delete_webhook".into() })
}
