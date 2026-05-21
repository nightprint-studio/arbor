/// Git remote URL utilities shared by multiple modules.
///
/// Centralises the two URL-transformation functions that were previously
/// duplicated between `auth/credential_store.rs` (`extract_host`) and
/// `commands/remote_commands.rs` (`normalize_remote_to_https`).

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
// Canonical key (fuzzy match)
// ---------------------------------------------------------------------------

/// Reduce any git remote URL to a canonical `host/owner/repo` key suitable
/// for *fuzzy* equality across schemes (https / ssh / scp-style), credentials,
/// `.git` suffix, trailing slashes and case differences.
///
/// Used by the deep-link router to find the local clone of a `arbor://`
/// target without forcing the link author to know which scheme the user
/// originally cloned with.
///
/// Examples — all return `Some("github.com/foo/bar")`:
///
/// ```text
/// https://github.com/foo/bar.git
/// https://github.com/foo/bar/
/// https://USER:token@github.com/Foo/Bar.git/
/// git@github.com:foo/bar.git
/// ssh://git@github.com:22/foo/bar
/// ```
///
/// Returns `None` when the URL doesn't yield a `(host, path)` pair (e.g.
/// `"not-a-url"`).  Path is preserved as-is *after* lowercasing — nested
/// groups (GitLab) are kept: `gitlab.com/group/sub/repo`.
pub fn canonical_key(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() { return None; }

    // Drop scheme, userinfo, port — produce a "host/path" string.
    let host_path: String = if let Some(idx) = s.find("://") {
        let after_scheme = &s[idx + 3..];
        let no_userinfo  = after_scheme.find('@')
            .map(|at| &after_scheme[at + 1..])
            .unwrap_or(after_scheme);
        no_userinfo.to_string()
    } else if let Some(at_idx) = s.find('@') {
        // scp-style:  user@host:path
        let after_user = &s[at_idx + 1..];
        match after_user.find(':') {
            Some(col) => format!("{}/{}", &after_user[..col], &after_user[col + 1..]),
            None      => after_user.to_string(),
        }
    } else {
        s.to_string()
    };

    let mut split = host_path.splitn(2, '/');
    let host_with_port = split.next()?;
    let path           = split.next()?;
    // Drop ":port" if present.
    let host = host_with_port.split(':').next()?.trim().to_lowercase();
    if host.is_empty() { return None; }

    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.is_empty() { return None; }

    Some(format!("{host}/{}", path.to_lowercase()))
}

// ---------------------------------------------------------------------------
// Probe `origin` URL from a local repo path
// ---------------------------------------------------------------------------

/// Best-effort lookup of `origin`'s URL for the repo at `path`, via the git
/// CLI.  Returns `None` when:
///   * `path` isn't a git repository
///   * the repo has no `origin` remote
///   * git itself isn't on PATH
///
/// Used to backfill `RepoRegistryEntry::remote_url` when a repo was registered
/// from disk (file-picker or "Open folder…") without the caller passing the
/// URL — which would otherwise leave the deep-link router unable to match
/// `arbor://…?url=…` to the local clone.
pub fn probe_origin_url(path: &std::path::Path) -> Option<String> {
    use crate::process_ext::NoWindowExt;
    let out = crate::git_cli::command()
        .args(["-C"])
        .arg(path)
        .args(["remote", "get-url", "origin"])
        .no_window()
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let url = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if url.is_empty() { None } else { Some(url) }
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
