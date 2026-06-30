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
/// APIs. Each process (shell / `corvus-be`) holds its own.
static CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

    let result = provider.avatar_url_for_email(trimmed).await.ok().flatten();
    if let Ok(mut c) = CACHE.lock() {
        c.insert(key, result.clone());
    }
    result
}
