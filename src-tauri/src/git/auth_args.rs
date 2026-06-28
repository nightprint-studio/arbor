//! Keyring-coupled HTTP auth-arg injection for git CLI shell-outs.
//!
//! Split out of `crate::git_cli` when detection/download moved to the Tauri-free
//! `corvus-git-cli` crate: this reads Arbor's stored OAuth token / PAT from the
//! keyring (`credential_store`) — shell-only state that never crosses into a
//! headless backend — so it stays here. Re-exported through `crate::git_cli` so
//! the `crate::git_cli::http_auth_args_for_url(s)` call sites are unchanged.

use crate::git::url::extract_host;

/// Look up the OAuth token / PAT Arbor has stored for the host of `url`,
/// regardless of whether the URL is HTTPS or SSH. Returns `(username, secret)`
/// where `username == "x-oauth-basic"` means the secret is an OAuth bearer
/// token, anything else is a PAT-style basic-auth pair.
fn token_for_url(url: &str) -> Option<(String, String)> {
    let host = extract_host(url)?;
    crate::auth::credential_store::resolve_credentials(&host).ok().flatten()
}

fn build_auth_header(username: &str, secret: &str, host: &str) -> String {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    // Neither GitHub nor GitLab accept `Authorization: Bearer …` for the
    // smart-HTTP git protocol (Bearer works only for REST APIs). Both want
    // HTTP Basic with a host-specific sentinel username:
    //   * GitHub → `x-access-token`
    //   * GitLab → `oauth2`   (any forge whose host starts with `gitlab.`)
    //   * other  → `x-access-token` (safe fallback; ignored by token-based forges)
    // For PAT-style credentials (username != "x-oauth-basic") the real
    // username is preserved.
    let user = if username == "x-oauth-basic" {
        if host == "gitlab.com" || host.starts_with("gitlab.") { "oauth2" } else { "x-access-token" }
    } else {
        username
    };
    let basic = B64.encode(format!("{user}:{secret}"));
    format!("Authorization: Basic {basic}")
}

/// Returns `https://host/` prefix that git's `http.<url>.<setting>` matching
/// uses to scope a config option to a specific host. Falls back to bare
/// scheme+host when no trailing slash is present.
fn url_match_prefix(url: &str) -> Option<String> {
    let host = extract_host(url)?;
    let scheme = if url.starts_with("https://") { "https" } else { "http" };
    Some(format!("{scheme}://{host}/"))
}

/// Build the global `-c` overrides that inject the right `Authorization` header
/// when shelling out to the git CLI for an HTTPS URL. Empty when:
///   - the URL isn't HTTP(S) (SSH falls back to ssh-agent / `~/.ssh/`),
///   - or Arbor has no stored token for that host.
///
/// Uses git's host-scoped form (`http.<https://host/>.extraHeader=…`) so the
/// token only travels to the matching host — important when an operation
/// (e.g. `submodule update --recursive`) might hit several remotes. Also clears
/// the credential-helper chain (host-scoped AND globally) so GCM / other helpers
/// don't pop a UI prompt or double-inject auth headers.
///
/// Auth scheme is always HTTP Basic — GitHub's git/HTTPS endpoint rejects
/// `Authorization: Bearer` for the smart-HTTP protocol (works for REST API
/// only), so we use `x-access-token:<token>` for OAuth tokens and the real
/// `<user>:<pat>` pair for PAT-style credentials.
///
/// Returns args to insert **before the subcommand**, e.g.
/// `git -c http.https://github.com/.extraHeader="Authorization: Basic …" clone <url>`.
///
/// IMPORTANT: the returned vector contains the secret token in plaintext.
/// Callers MUST NOT log it, splice it into job-display strings, or surface it in
/// error messages. The `Command` itself is fine because Tauri / OS don't echo
/// argv to the user.
pub fn http_auth_args_for_url(url: &str) -> Vec<String> {
    http_auth_args_for_urls(std::slice::from_ref(&url.to_string()))
}

/// Like [`http_auth_args_for_url`] but for several URLs that may span multiple
/// hosts (typical for `git submodule update --recursive`). One host-scoped `-c`
/// pair per known host with a stored Arbor token; URLs without a token, plus
/// SSH/file URLs, are silently skipped. Duplicates (multiple URLs with the same
/// host) emit a single config entry.
pub fn http_auth_args_for_urls(urls: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut seen_hosts: BTreeSet<String> = BTreeSet::new();
    let mut args: Vec<String> = Vec::new();

    for url in urls {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            continue;
        }
        let Some(host) = extract_host(url) else { continue; };
        if !seen_hosts.insert(host.clone()) {
            continue;
        }
        let Some(prefix) = url_match_prefix(url) else { continue; };
        let Some((username, secret)) = token_for_url(url) else { continue; };
        let header = build_auth_header(&username, &secret, &host);
        args.push("-c".into());
        args.push(format!("http.{prefix}.extraHeader={header}"));
        // Reset the credential-helper chain for this URL so GCM (or any other
        // helper configured globally in ~/.gitconfig) doesn't ALSO inject its
        // own Authorization header — duplicate auth makes GitHub return 400, and
        // an eager helper invocation can show a UI prompt that freezes the
        // WebView until dismissed. The correct namespace is `credential.*`, NOT
        // `http.*` (which has no `helper` option — the previous form was a silent
        // no-op). We set both the host-scoped form and a global empty value
        // because some helpers ignore the URL-scoped reset.
        args.push("-c".into());
        args.push(format!("credential.{prefix}.helper="));
    }
    if !args.is_empty() {
        args.push("-c".into());
        args.push("credential.helper=".into());
    }
    args
}
