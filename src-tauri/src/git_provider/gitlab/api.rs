//! GitLab HTTP helpers — base URL + token retrieval.
//!
//! `gitlab_send_with_refresh` lives in `crate::git_provider::ci_impl` and is
//! re-exported here for the duration of the Phase 3/5 transition. Phase 5
//! will move it physically.

pub const GITLAB_COM_WEB:  &str = "https://gitlab.com";
pub const USER_AGENT:      &str = "arbor-git-gui/1.0";

/// Lazily fetch the stored GitLab token (OAuth on gitlab.com, host-based PAT
/// on self-hosted). Delegates to the credential-store wrapper that already
/// exists in `pipeline::ci_client`.
pub fn get_token(base_url: &str) -> crate::error::Result<Option<String>> {
    crate::git_provider::ci_impl::get_gitlab_token(base_url)
}

pub(crate) use crate::git_provider::ci_impl::gitlab_send_with_refresh;

/// Percent-encode `/` as `%2F` for GitLab project path in URL segments.
/// Mirrors the helper used inside `pipeline::ci_client`.
pub fn percent_encode_slash(s: &str) -> String {
    s.replace('/', "%2F")
}
