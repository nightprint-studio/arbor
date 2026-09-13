//! Cached commit-email → avatar resolution, shared by the shell and the OOP
//! backend so the REST lookup itself lives once (in each provider's
//! [`GitProvider::avatar_url_for_email`]).
//!
//! This wrapper adds the process-local memo + machine-email skip that both
//! call sites need; the per-provider REST (GitHub `noreply`/search, GitLab
//! `?search=`) stays in the impl crates.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::provider::GitProvider;

/// Per-process cache keyed by `(host, lowercased email)`, **including negative
/// results**, so re-rendering the same commit graph never re-hits the search
/// APIs. Each process (shell / `corvus-be`) holds its own. Bounded by
/// [`CAPACITY`]; a failed lookup is not an answer and is never stored.
static CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The most lookups the cache keeps. Far above the authors of any one graph, so a
/// repository's avatars stay warm; what it stops is a long session across many
/// repositories growing the map for as long as the process lives.
const CAPACITY: usize = 4096;

/// How many `(host, email)` lookups the cache holds, misses included — for a backend's memory
/// breakdown.
pub fn avatar_cache_len() -> usize {
    CACHE.lock().map(|c| c.len()).unwrap_or(0)
}

/// Store a lookup, making room first when the cache is full: the misses go before the hits —
/// a miss costs one search to learn again, and is the likelier of the two to be stale (an
/// author who has since set an avatar) — and only if that is not enough does everything go.
fn remember(cache: &mut HashMap<String, Option<String>>, key: String, value: Option<String>) {
    if cache.len() >= CAPACITY && !cache.contains_key(&key) {
        cache.retain(|_, v| v.is_some());
        if cache.len() >= CAPACITY {
            cache.clear();
        }
    }
    cache.insert(key, value);
}

/// Resolve an `avatar_url` for `email` via `provider`, memoised by `host` (the
/// repo's remote URL — the cache scope). Fully best-effort: an empty/machine
/// email or any provider error resolves to `None`, and the caller falls back to
/// a generated initials avatar.
pub async fn resolve_avatar(provider: &dyn GitProvider, host: &str, email: &str) -> Option<String> {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Skip well-known machine emails entirely.
    if trimmed.eq_ignore_ascii_case("noreply@github.com") {
        return None;
    }

    let key = format!("{host}::{}", trimmed.to_lowercase());
    if let Some(cached) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return cached;
    }

    // An error (offline, rate-limited, a token that expired) says nothing about the
    // author, so it is not remembered: caching it as a miss would hide this author's
    // avatar for the rest of the session, even after the network came back.
    let result = provider.avatar_url_for_email(trimmed).await.ok()?;
    if let Ok(mut c) = CACHE.lock() {
        remember(&mut c, key, result.clone());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_cache_drops_its_misses_before_its_hits() {
        let mut cache = HashMap::new();
        for i in 0..CAPACITY {
            let value = (i % 2 == 0).then(|| format!("url{i}"));
            cache.insert(format!("k{i}"), value);
        }
        remember(&mut cache, "new".to_string(), None);
        assert_eq!(cache.len(), CAPACITY / 2 + 1);
        assert!(cache.values().filter(|v| v.is_none()).count() == 1, "only the new miss");
    }

    #[test]
    fn a_cache_full_of_hits_starts_over() {
        let mut cache: HashMap<String, Option<String>> =
            (0..CAPACITY).map(|i| (format!("k{i}"), Some(String::new()))).collect();
        remember(&mut cache, "new".to_string(), Some("u".to_string()));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn updating_a_known_key_never_evicts() {
        let mut cache: HashMap<String, Option<String>> =
            (0..CAPACITY).map(|i| (format!("k{i}"), None)).collect();
        remember(&mut cache, "k0".to_string(), Some("u".to_string()));
        assert_eq!(cache.len(), CAPACITY);
    }
}
