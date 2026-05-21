//! Provider registry — host → `Arc<dyn GitProvider>` lookup.
//!
//! Populated at app boot (Phase 4 wires this) with one provider per
//! registered host: always `github.com`, always `gitlab.com`, plus any
//! self-hosted GitLab instance discovered through `credential_store`.
//!
//! Phase 1: registry exists and is empty. The convenience helpers below
//! (`for_active_tab`, `for_remote_url`) return `None` until populated.

use std::collections::HashMap;
use std::sync::Arc;

use super::GitProvider;

/// In-memory map keyed by hostname (lowercased).
#[derive(Default)]
pub struct GitProviderRegistry {
    by_host: HashMap<String, Arc<dyn GitProvider>>,
}

impl GitProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a provider keyed by its `host()`.
    pub fn register(&mut self, provider: Arc<dyn GitProvider>) {
        let host = provider.host().to_lowercase();
        self.by_host.insert(host, provider);
    }

    /// Lookup by hostname (case-insensitive).
    pub fn for_host(&self, host: &str) -> Option<Arc<dyn GitProvider>> {
        self.by_host.get(&host.to_lowercase()).cloned()
    }

    /// Lookup by parsing the host out of a remote URL (HTTPS or SSH).
    /// Returns `None` when the URL doesn't have a recognizable host or
    /// when no provider is registered for that host.
    pub fn for_remote_url(&self, url: &str) -> Option<Arc<dyn GitProvider>> {
        let host = host_from_url(url)?;
        self.for_host(&host)
    }

    /// All currently registered providers (iteration order unspecified).
    pub fn list(&self) -> Vec<Arc<dyn GitProvider>> {
        self.by_host.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.by_host.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_host.is_empty()
    }
}

/// Parse the hostname out of an HTTPS or SSH-style git remote URL.
/// `None` for unrecognizable shapes.
fn host_from_url(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        let end = rest.find('/').unwrap_or(rest.len());
        let host = &rest[..end];
        // Strip optional `user@host` form (rare for HTTPS but tolerated).
        let host = host.rsplit('@').next().unwrap_or(host);
        if host.is_empty() { return None; }
        return Some(host.to_string());
    }
    if let Some(rest) = url.strip_prefix("git@") {
        // git@github.com:owner/repo.git → host = github.com
        let colon = rest.find(':')?;
        let host = &rest[..colon];
        if host.is_empty() { return None; }
        return Some(host.to_string());
    }
    if let Some(rest) = url.strip_prefix("ssh://git@") {
        let end = rest.find(|c: char| c == '/' || c == ':').unwrap_or(rest.len());
        let host = &rest[..end];
        if host.is_empty() { return None; }
        return Some(host.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::host_from_url;

    #[test]
    fn parses_https() {
        assert_eq!(host_from_url("https://github.com/foo/bar.git").as_deref(), Some("github.com"));
    }
    #[test]
    fn parses_ssh_short() {
        assert_eq!(host_from_url("git@gitlab.com:foo/bar.git").as_deref(), Some("gitlab.com"));
    }
    #[test]
    fn parses_ssh_long() {
        assert_eq!(host_from_url("ssh://git@github.example.org/foo/bar").as_deref(), Some("github.example.org"));
    }
    #[test]
    fn rejects_garbage() {
        assert_eq!(host_from_url("not a url"), None);
    }
}
