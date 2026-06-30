//! Git remote URL utilities shared by multiple modules.
//!
//! Centralises the two URL-transformation functions that were previously
//! duplicated between `auth/credential_store.rs` (`extract_host`) and
//! `commands/remote_commands.rs` (`normalize_remote_to_https`).

// ---------------------------------------------------------------------------
// Host extraction
// ---------------------------------------------------------------------------

/// Extract the bare hostname from HTTPS, HTTP, or SSH (`git@host:path`) URLs.
///
/// Examples:
/// ```text
/// "https://github.com/owner/repo.git" → Some("github.com")
/// "git@github.com:owner/repo.git"     → Some("github.com")
/// "not-a-url"                          → None
/// ```
pub fn extract_host(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        // Skip optional "user:pass@" or "user@" auth prefix
        let after_at = rest.find('@').map(|i| &rest[i + 1..]).unwrap_or(rest);
        // Drop port and path, keep hostname only
        let host = after_at.split('/').next()?.split(':').next()?;
        return Some(host.to_string());
    }
    if let Some(rest) = url.strip_prefix("git@") {
        // git@github.com:org/repo.git  →  github.com
        let host = rest.split(':').next()?;
        return Some(host.to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// HTTPS normalisation
// ---------------------------------------------------------------------------

/// Convert any git remote URL to a plain HTTPS URL (without `.git` suffix).
///
/// Handles:
/// - SSH:   `git@github.com:owner/repo.git` → `https://github.com/owner/repo`
/// - HTTPS: `https://github.com/owner/repo.git` → `https://github.com/owner/repo`
///
/// Returns `None` when the URL cannot be recognised as a git remote URL.
pub fn normalize_to_https(url: &str) -> Option<String> {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        let path = path.strip_suffix(".git").unwrap_or(path);
        return Some(format!("https://{host}/{path}"));
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        let url = url.strip_suffix(".git").unwrap_or(url);
        return Some(url.to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// Forge URL builder
// ---------------------------------------------------------------------------

/// Build a forge-specific URL from a normalised HTTPS base and a target string.
///
/// Targets: `"repo"` | `"commit:{oid}"` | `"branch:{name}"` | `"tag:{name}"`
pub fn forge_url(base: &str, target: &str) -> String {
    if target == "repo" {
        return base.to_string();
    }
    let Some((kind, value)) = target.split_once(':') else {
        return base.to_string();
    };
    if base.contains("github.com") {
        return match kind {
            "commit" => format!("{base}/commit/{value}"),
            "branch" => format!("{base}/tree/{value}"),
            "tag"    => format!("{base}/releases/tag/{value}"),
            _        => base.to_string(),
        };
    }
    if base.contains("gitlab.com") || base.contains("gitlab.") {
        return match kind {
            "commit" => format!("{base}/-/commit/{value}"),
            "branch" => format!("{base}/-/tree/{value}"),
            "tag"    => format!("{base}/-/tags/{value}"),
            _        => base.to_string(),
        };
    }
    if base.contains("bitbucket.org") {
        return match kind {
            "commit" => format!("{base}/commits/{value}"),
            "branch" => format!("{base}/branch/{value}"),
            "tag"    => format!("{base}/src/{value}"),
            _        => base.to_string(),
        };
    }
    // Unknown forge — return the repo root
    base.to_string()
}
