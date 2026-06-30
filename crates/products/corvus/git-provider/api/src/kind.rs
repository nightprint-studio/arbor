use serde::{Deserialize, Serialize};

/// Identity tag for a provider implementation. Echoed in `MrId` and other
/// provider-aware payloads so the frontend can branch on origin without
/// having to compare strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    GitHub,
    GitLab,
    /// Reserved for future implementations.
    Gitea,
    Bitbucket,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::GitHub    => "github",
            ProviderKind::GitLab    => "gitlab",
            ProviderKind::Gitea     => "gitea",
            ProviderKind::Bitbucket => "bitbucket",
        }
    }
}
