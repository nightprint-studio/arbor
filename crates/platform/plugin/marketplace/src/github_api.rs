//! Shared GitHub-API surface for the marketplace.
//!
//! Centralizes everything the fetcher and the installer used to duplicate
//! between them:
//!
//!   * the HTTP client builder (used to be re-built with slightly different
//!     timeouts / UAs in two places),
//!   * URL composition for the raw host (`raw.githubusercontent.com`),
//!     the canonical `github.com/<owner>/<repo>` form, and the archive
//!     download URL,
//!   * `parse_github_repo` URL parser,
//!   * a single `resolve_ref_sha` API call that returns the SHA a ref
//!     points at — used by both the install path (recording
//!     `resolved_sha`) and the pin-verify path (refusing entries whose
//!     SHA doesn't match the manifest pin).
//!
//! Both call sites used to inline `struct Resp { sha: String }` + their
//! own GET to `/repos/{owner}/{repo}/commits/{ref}`; that duplication is
//! now gone.

use std::time::Duration;

use serde::Deserialize;

use crate::error::{MarketplaceError, Result};

/// Per-request timeout — generous enough for a slow GitHub edge but short
/// enough that the modal doesn't feel stuck.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

/// `raw.githubusercontent.com` host. Public so the catalog fetcher and
/// the installer can compose their own URLs without re-stringifying it.
pub const RAW_HOST: &str = "https://raw.githubusercontent.com";

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

/// Marketplace-specific HTTP client. Same UA + timeout pair used to live in
/// `fetcher::client()` and was re-used at call sites by importing it across
/// modules; lives here now as the single source of truth.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("arbor/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(MarketplaceError::from)
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Parse `https://github.com/{owner}/{repo}[.git]` → `(owner, repo)`. Lenient
/// on trailing slash / `.git` suffix / `http` vs `https`. Returns `None`
/// for anything that isn't a recognisable GitHub URL.
pub fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let stripped = url.trim_end_matches('/').trim_end_matches(".git");
    let suffix = stripped
        .strip_prefix("https://github.com/")
        .or_else(|| stripped.strip_prefix("http://github.com/"))?;
    let mut parts = suffix.split('/');
    let owner = parts.next()?;
    let repo  = parts.next()?;
    if owner.is_empty() || repo.is_empty() { return None; }
    Some((owner.to_string(), repo.to_string()))
}

/// Canonical-form a GitHub URL so two strings that point at the same repo
/// compare equal — drops `.git`, the trailing slash, `http`-vs-`https`
/// skew, and the casing of the host. Returns `None` for anything that
/// isn't a recognisable GitHub URL.
pub fn normalise_github_url(url: &str) -> Option<String> {
    let (owner, repo) = parse_github_repo(url)?;
    Some(github_url(&owner, &repo))
}

/// Canonical `https://github.com/{owner}/{repo}` form. Used for
/// `MarketplacePlugin::repository` defaults and `RegistryEntry::repo`
/// roundtrips so the wire shape stays uniform.
pub fn github_url(owner: &str, repo: &str) -> String {
    format!("https://github.com/{owner}/{repo}")
}

/// Compose a `raw.githubusercontent.com` URL: `{owner}/{repo}/{ref}/{path}`.
pub fn raw_url(owner: &str, repo: &str, r#ref: &str, path: &str) -> String {
    let p = path.trim_start_matches('/');
    format!("{RAW_HOST}/{owner}/{repo}/{}/{p}", r#ref)
}

/// Compose the zipball download URL: `archive/{ref}.zip` under the
/// canonical `github.com/{owner}/{repo}` path.
pub fn archive_url(owner: &str, repo: &str, r#ref: &str) -> String {
    format!("https://github.com/{owner}/{repo}/archive/{}.zip", r#ref)
}

/// Join a repo-relative subpath with a file leaf, handling empty / leading
/// / trailing slashes uniformly. `"" + "plugin.toml" → "plugin.toml"`,
/// `"a/b/" + "/plugin.toml" → "a/b/plugin.toml"`.
/// Direct download URL for a file attached to a release.
///
/// The plain `releases/download` form rather than the REST API: it needs no token, no
/// second round trip to list the assets, and it 404s cleanly when a release does not carry
/// what the registry said it would — which is the same failure the API would report, one
/// request earlier.
///
/// A **tag** rather than a ref in general. A release belongs to a tag by construction, and an
/// entry that records artifact digests cannot ride a branch anyway: the digests would be
/// pinned to a target that moves.
pub fn release_asset_url(owner: &str, repo: &str, tag: &str, asset: &str) -> String {
    format!("https://github.com/{owner}/{repo}/releases/download/{tag}/{asset}")
}

pub fn join_subpath(subpath: &str, file: &str) -> String {
    let s = subpath.trim_end_matches('/');
    let f = file.trim_start_matches('/');
    if s.is_empty() { f.to_string() } else { format!("{s}/{f}") }
}

// ---------------------------------------------------------------------------
// `/repos/{owner}/{repo}/commits/{ref}` — resolve + pin-verify
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CommitResp { sha: String }

/// Resolve the actual commit SHA the ref currently points at via the
/// unauthenticated GitHub commits endpoint. The same call is used by both
/// the install path (recording `resolved_sha` as a best-effort fingerprint)
/// and the pin-verify path.
pub async fn resolve_ref_sha(
    http:  &reqwest::Client,
    owner: &str,
    repo:  &str,
    r#ref: &str,
) -> Result<String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{}", r#ref);
    let r: CommitResp = http.get(&url)
        .header("Accept", "application/vnd.github+json")
        .send().await
        .map_err(|e| MarketplaceError::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {url}: {e}")))?
        .json().await
        .map_err(|e| MarketplaceError::Other(format!("parse {url}: {e}")))?;
    Ok(r.sha)
}

/// Refuse to continue if the resolved SHA disagrees with `pinned`. Pins are
/// compared prefix-insensitively to allow short pins (≥7 hex chars).
pub async fn verify_pinned_sha(
    http:    &reqwest::Client,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    pinned:  &str,
) -> Result<()> {
    let sha = resolve_ref_sha(http, owner, repo, r#ref).await?;
    let pin_norm = pinned.trim().to_lowercase();
    let sha_norm = sha.to_lowercase();
    if pin_norm.len() < 7 {
        return Err(MarketplaceError::PinMismatch(format!(
            "pinned_sha '{pinned}' is too short (need ≥7 hex chars)"
        )));
    }
    if !sha_norm.starts_with(&pin_norm) {
        return Err(MarketplaceError::PinMismatch(format!(
            "{owner}/{repo}@{}: expected '{pinned}', got '{}'",
            r#ref, &sha_norm[..pin_norm.len().min(sha_norm.len())],
        )));
    }
    Ok(())
}
