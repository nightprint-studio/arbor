//! Provider detection from a remote URL.
//!
//! Behavior parity with `pipeline::ci_client::detect_from_url` — Phase 5
//! deletes that function once every consumer goes through this module.

use super::ProviderKind;

/// Best-effort provider detection from a single remote URL.
///
/// Recognizes:
/// - github.com  → `ProviderKind::GitHub`
/// - gitlab.com  → `ProviderKind::GitLab`
/// - any host containing `gitlab.` → `ProviderKind::GitLab` (self-hosted)
#[allow(dead_code)]
pub fn detect_from_remote_url(url: &str) -> Option<ProviderKind> {
    if url.contains("github.com") {
        return Some(ProviderKind::GitHub);
    }
    if url.contains("gitlab.com") || url.contains("gitlab.") {
        return Some(ProviderKind::GitLab);
    }
    None
}

/// Convenience: pick the first recognizable remote, preferring `origin`.
#[allow(dead_code)]
pub fn detect_from_remotes(remotes: &[(String, String)]) -> Option<ProviderKind> {
    let ordered = remotes.iter()
        .filter(|(n, _)| n == "origin")
        .chain(remotes.iter().filter(|(n, _)| n != "origin"));
    for (_, url) in ordered {
        if let Some(k) = detect_from_remote_url(url) {
            return Some(k);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_github() {
        assert_eq!(detect_from_remote_url("https://github.com/a/b.git"), Some(ProviderKind::GitHub));
        assert_eq!(detect_from_remote_url("git@github.com:a/b.git"),     Some(ProviderKind::GitHub));
    }

    #[test]
    fn detects_gitlab_com() {
        assert_eq!(detect_from_remote_url("https://gitlab.com/a/b.git"), Some(ProviderKind::GitLab));
    }

    #[test]
    fn detects_self_hosted_gitlab() {
        assert_eq!(detect_from_remote_url("https://gitlab.example.org/a/b.git"), Some(ProviderKind::GitLab));
    }

    #[test]
    fn rejects_other() {
        assert_eq!(detect_from_remote_url("https://bitbucket.org/a/b.git"), None);
    }
}
