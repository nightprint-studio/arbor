use serde::{Deserialize, Serialize};

/// Authentication state echoed to the frontend.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuth {
    pub has_token: bool,
    pub user:      Option<ProviderUser>,
}

/// Identity of the authenticated user on a provider.
///
/// Field set is the intersection of GitHub + GitLab user payloads — keep it
/// minimal so adding Gitea / Bitbucket later doesn't force breaking changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUser {
    pub id:         String,
    pub login:      String,
    pub name:       Option<String>,
    pub email:      Option<String>,
    pub avatar_url: Option<String>,
    pub web_url:    Option<String>,
}

/// In-flight OAuth handle returned by `start_oauth`. Held by the caller
/// (frontend) and passed back to `complete_oauth` along with the auth code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthHandle {
    /// Provider-issued state token, also used to verify the redirect.
    pub state:        String,
    /// Local listener port the provider redirects back to.
    pub callback_port: u16,
    /// PKCE verifier (kept opaque to the frontend).
    pub pkce_verifier: String,
    /// Authorization URL the user must open in their browser.
    pub auth_url:     String,
}
