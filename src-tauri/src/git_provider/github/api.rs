//! GitHub HTTP helpers — base URL constants + token retrieval.
//!
//! The actual `github_send_with_refresh` lives in
//! `crate::git_provider::ci_impl` and is reached via a `pub(crate)` re-export
//! during the Phase 2/3 transition. Phase 5 will move it physically.

pub const GITHUB_API_BASE: &str = "https://api.github.com";
pub const GITHUB_WEB_BASE: &str = "https://github.com";
pub const USER_AGENT:      &str = "arbor-git-gui/1.0";
pub const ACCEPT_JSON:     &str = "application/vnd.github+json";
pub const API_VERSION:     &str = "2022-11-28";

/// Lazily fetch the stored GitHub token (OAuth or PAT).
///
/// Delegates to the credential-store wrapper that already exists in
/// `pipeline::ci_client`. Phase 5 will own this lookup directly.
pub fn get_token() -> crate::error::Result<Option<String>> {
    crate::git_provider::ci_impl::get_github_token()
}

pub(crate) use crate::git_provider::ci_impl::github_send_with_refresh;
