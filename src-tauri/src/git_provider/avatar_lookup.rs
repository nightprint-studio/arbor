//! Provider-driven avatar resolution.
//!
//! Looks up a remote user by commit email and returns their `avatar_url`
//! when the active provider (GitHub / GitLab) can match. Used by the
//! commit-graph avatar layer (`src/lib/stores/avatars.svelte.ts`) so the
//! viewer sees real platform avatars instead of generic initials whenever
//! the commit email is exposed on the platform.
//!
//! Results are cached per-process keyed by `(host, lowercased email)` —
//! including negative results — so re-rendering the same graph never
//! re-hits the search APIs.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::Deserialize;

use crate::git_provider::{
    github::api as gh_api,
    gitlab::api as gl_api,
    helpers::ResolvedProvider,
};

static CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache_key(host: &str, email: &str) -> String {
    format!("{host}::{}", email.trim().to_lowercase())
}

fn cache_get(key: &str) -> Option<Option<String>> {
    CACHE.lock().ok().and_then(|c| c.get(key).cloned())
}

fn cache_put(key: String, value: Option<String>) {
    if let Ok(mut c) = CACHE.lock() {
        c.insert(key, value);
    }
}

/// Resolve an `avatar_url` for `email` using the provider bound to this
/// repo. `None` means "no match" (or the provider can't help) — the
/// frontend then falls back to a generated initials avatar.
pub async fn resolve_for(resolved: &ResolvedProvider, email: &str) -> Option<String> {
    let trimmed = email.trim();
    if trimmed.is_empty() { return None; }

    // Skip well-known machine emails entirely.
    if trimmed.eq_ignore_ascii_case("noreply@github.com") { return None; }

    let host = resolved.info.remote_url.as_str();
    let key  = cache_key(host, trimmed);
    if let Some(cached) = cache_get(&key) { return cached; }

    let result = match resolved.info.provider.as_str() {
        "github" => fetch_github(trimmed).await,
        "gitlab" => fetch_gitlab(resolved, trimmed).await,
        _ => None,
    };
    cache_put(key, result.clone());
    result
}

// ── GitHub ───────────────────────────────────────────────────────────────────

/// Parses GitHub's `users.noreply.github.com` commit emails so we can hit
/// the much cheaper `GET /users/:username` endpoint instead of the rate-
/// limited search API. Two forms exist:
///   `<id>+<username>@users.noreply.github.com`
///   `<username>@users.noreply.github.com`
fn parse_github_noreply(email: &str) -> Option<String> {
    let lower = email.to_ascii_lowercase();
    let (local, _) = lower.split_once('@')?;
    if !lower.ends_with("@users.noreply.github.com") { return None; }
    let username = match local.split_once('+') {
        Some((_, name)) => name,
        None            => local,
    };
    if username.is_empty() { None } else { Some(username.to_string()) }
}

async fn fetch_github(email: &str) -> Option<String> {
    let token = gh_api::get_token().ok().flatten()?;
    let client = reqwest::Client::new();

    // Fast path: noreply emails encode the username directly.
    if let Some(username) = parse_github_noreply(email) {
        let url = format!("{}/users/{}", gh_api::GITHUB_API_BASE, percent_encode(&username));
        let resp = gh_api::github_send_with_refresh(
            |tok| client.get(&url)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Accept", gh_api::ACCEPT_JSON)
                .header("X-GitHub-Api-Version", gh_api::API_VERSION)
                .header("User-Agent", gh_api::USER_AGENT),
            &token,
        ).await.ok()?;
        if !resp.status().is_success() { return None; }
        #[derive(Deserialize)]
        struct U { avatar_url: Option<String> }
        let u: U = resp.json().await.ok()?;
        return u.avatar_url;
    }

    // Slow path: search by public email. Only matches users who have made
    // the email public on their profile — coverage is partial by design.
    let q = format!("{}+in:email", email);
    let url = format!(
        "{}/search/users?q={}&per_page=1",
        gh_api::GITHUB_API_BASE,
        percent_encode(&q),
    );
    let resp = gh_api::github_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", gh_api::ACCEPT_JSON)
            .header("X-GitHub-Api-Version", gh_api::API_VERSION)
            .header("User-Agent", gh_api::USER_AGENT),
        &token,
    ).await.ok()?;
    if !resp.status().is_success() { return None; }

    #[derive(Deserialize)]
    struct SearchResp { items: Vec<U> }
    #[derive(Deserialize)]
    struct U { avatar_url: Option<String> }

    let body: SearchResp = resp.json().await.ok()?;
    body.items.into_iter().next().and_then(|u| u.avatar_url)
}

// ── GitLab ───────────────────────────────────────────────────────────────────

async fn fetch_gitlab(resolved: &ResolvedProvider, email: &str) -> Option<String> {
    let base = resolved.info.gitlab_base_url.as_deref()
        .unwrap_or(gl_api::GITLAB_COM_WEB)
        .trim_end_matches('/');
    let token = gl_api::get_token(base).ok().flatten()?;

    // GitLab's `?search=` matches name, username and *public* email.
    let url = format!(
        "{base}/api/v4/users?search={}&per_page=1",
        percent_encode(email),
    );
    let client = reqwest::Client::new();
    let resp = gl_api::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("PRIVATE-TOKEN", tok)
            .header("User-Agent", gl_api::USER_AGENT),
        base,
        &token,
    ).await.ok()?;
    if !resp.status().is_success() { return None; }

    #[derive(Deserialize)]
    struct U { avatar_url: Option<String> }
    let body: Vec<U> = resp.json().await.ok()?;
    body.into_iter().next().and_then(|u| u.avatar_url)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let safe = matches!(b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~'
        );
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
