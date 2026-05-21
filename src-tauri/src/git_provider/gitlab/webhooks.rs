//! GitLab webhooks — STUB. Every method returns `Unsupported`.

use crate::git_provider::types::{
    Webhook, WebhookCreateRequest, RepoRef, error::ProviderError,
};

pub async fn list_webhooks(_repo: &RepoRef) -> Result<Vec<Webhook>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_webhooks".into() })
}

pub async fn create_webhook(
    _repo: &RepoRef,
    _req:  WebhookCreateRequest,
) -> Result<Webhook, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_webhook".into() })
}

pub async fn delete_webhook(_repo: &RepoRef, _id: &str) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "delete_webhook".into() })
}
