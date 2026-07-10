//! Credential store — a single OS-keychain item backing *all* of Arbor's stored
//! secrets (git-provider OAuth tokens & PATs, refresh tokens, Jira/Linear keys).
//!
//! # Why one item
//!
//! macOS pops a Keychain authorisation dialog on *every* item read whose ACL
//! doesn't "Always Allow" the running binary — and for an unsigned / ad-hoc dev
//! build whose code signature changes on each compile, that's every read. With
//! one keychain item per credential, a couple of git operations turned into a
//! wall of password prompts (one per distinct credential, plus the startup token
//! probes). Windows never re-prompts a process already granted, hence the gap.
//!
//! So everything now lives in a **single** item ([`SERVICE`] / [`VAULT_ACCOUNT`])
//! holding a JSON object `{ account -> secret }`. The first credential access in
//! a process reads that one item (the single prompt) and mirrors it into an
//! in-memory map; every `get`/`save`/`delete` afterwards is pure memory plus, on
//! writes, one keychain write. Net effect: **at most one read prompt per
//! session** (and with a stable code signature, "Always Allow" on that one item
//! sticks across rebuilds).
//!
//! This module is the sole gate to the store: every write goes through
//! `save`/`save_for_host`, every read through `get`/`get_for_host`/
//! `resolve_credentials`, so the in-memory map is authoritative — the shell is
//! the only writer, so there is nothing external to invalidate against. The
//! public API is unchanged; only the backing storage moved from N items to one.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use keyring::Entry;

use crate::error::{AppError, Result};

/// Keychain service — shows as the item name in Keychain Access.
const SERVICE: &str = "Arbor";
/// The single item's account: `Arbor` / `credentials`.
const VAULT_ACCOUNT: &str = "credentials";

// ── In-memory vault ────────────────────────────────────────────────────────────

/// In-memory mirror of the vault. `None` until the first access, `Some(map)`
/// once loaded — the map is the in-process source of truth (the shell is the
/// sole writer, so we never reload). Keys are credential "accounts"
/// (`github.com/arbor`, a bare host, `github.com/arbor-refresh`, Jira keys, …);
/// values are the raw secrets (`get_for_host` stores/parses `"user\tpass"`).
static VAULT: LazyLock<Mutex<Option<HashMap<String, String>>>> =
    LazyLock::new(|| Mutex::new(None));

/// Read + parse the vault item from the OS store. A missing item or an
/// unparseable blob yields an empty map — a corrupt vault just means re-auth,
/// never a crash. This is the *only* function that reads the keychain (hence the
/// single prompt).
fn read_vault_from_store() -> Result<HashMap<String, String>> {
    let entry = Entry::new(SERVICE, VAULT_ACCOUNT)
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.get_password() {
        Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        Err(keyring::Error::NoEntry) => Ok(HashMap::new()),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

/// Persist the whole map back to the single keychain item (one write).
fn write_vault_to_store(map: &HashMap<String, String>) -> Result<()> {
    let json = serde_json::to_string(map)
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    let entry = Entry::new(SERVICE, VAULT_ACCOUNT)
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    entry
        .set_password(&json)
        .map_err(|e| AppError::AuthFailed(e.to_string()))
}

/// Look up one account in the vault, loading it from the store on first use.
fn read_vault(key: &str) -> Result<Option<String>> {
    let mut guard = VAULT.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        *guard = Some(read_vault_from_store()?);
    }
    Ok(guard.as_ref().expect("vault loaded above").get(key).cloned())
}

/// Mutate the vault (loading it on first use) and persist the result. The whole
/// read-modify-write runs under the lock so concurrent saves serialise.
fn mutate_vault(f: impl FnOnce(&mut HashMap<String, String>)) -> Result<()> {
    let mut guard = VAULT.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        *guard = Some(read_vault_from_store()?);
    }
    let map = guard.as_mut().expect("vault loaded above");
    f(map);
    write_vault_to_store(map)
}

// ── Per-account credential (OAuth tokens, refresh tokens, Jira/Linear keys) ─────

/// Save (or update) a credential under `host` (an opaque account key).
pub fn save(host: &str, _username: &str, password: &str) -> Result<()> {
    let (host, password) = (host.to_string(), password.to_string());
    mutate_vault(move |m| {
        m.insert(host, password);
    })
}

/// Retrieve a stored credential. Returns `None` if not present.
pub fn get(host: &str, _username: &str) -> Result<Option<String>> {
    read_vault(host)
}

/// Delete a stored credential.
pub fn delete(host: &str, _username: &str) -> Result<()> {
    let host = host.to_string();
    mutate_vault(move |m| {
        m.remove(&host);
    })
}

// ---------------------------------------------------------------------------
// Host-based default credential (used for automatic fetch/push auth)
//
// Stores a single "default" credential per hostname so that network operations
// can look up credentials without knowing the username in advance.
// Vault key: the bare host — value: "username\ttoken_or_password".
// ---------------------------------------------------------------------------

/// Save or replace the default credential for a host/URL.
pub fn save_for_host(url_or_host: &str, username: &str, password: &str) -> Result<()> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    let combined = format!("{username}\t{password}");
    mutate_vault(move |m| {
        m.insert(host, combined);
    })
}

/// Retrieve the default (username, password/token) for a host/URL.
pub fn get_for_host(url_or_host: &str) -> Result<Option<(String, String)>> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    Ok(read_vault(&host)?.and_then(|s| {
        s.split_once('\t')
            .map(|(user, pass)| (user.to_string(), pass.to_string()))
    }))
}

/// Delete the default credential for a host/URL.
pub fn delete_for_host(url_or_host: &str) -> Result<()> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    mutate_vault(move |m| {
        m.remove(&host);
    })
}

// ---------------------------------------------------------------------------
// Unified credential resolution (used by git remote operations)
//
// Priority:
//   1. OAuth token stored by Device Flow: key = "{host}/arbor"
//   2. Default (PAT / username+password) stored via Settings → Credentials
//
// Returns (username, password/token) or None if nothing is stored.
// For OAuth tokens, the username is "x-oauth-basic" (accepted by GitHub and GitLab).
// ---------------------------------------------------------------------------

pub fn resolve_credentials(url_or_host: &str) -> Result<Option<(String, String)>> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());

    // 1. OAuth token (Device Flow)
    let oauth_key = format!("{host}/arbor");
    if let Some(token) = read_vault(&oauth_key)? {
        return Ok(Some(("x-oauth-basic".to_string(), token)));
    }

    // 2. PAT / username+password (Settings → Credentials)
    get_for_host(url_or_host)
}

// ---------------------------------------------------------------------------
// URL → hostname extraction (re-exported from git::url for convenience)
// ---------------------------------------------------------------------------

/// Extract the bare hostname from HTTPS, HTTP, or SSH (git@host:path) URLs.
/// Delegates to [`crate::git::url::extract_host`].
pub fn extract_host(url: &str) -> Option<String> {
    crate::git::url::extract_host(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Seeding the in-memory vault sets it to `Some(..)`, so subsequent reads
    // never touch the OS keychain — the parsing / priority logic is exercised
    // without a live store. Each test uses unique keys to stay parallel-safe.
    fn seed(pairs: &[(&str, &str)]) {
        let mut g = VAULT.lock().unwrap_or_else(|p| p.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        for (k, v) in pairs {
            m.insert((*k).to_string(), (*v).to_string());
        }
    }

    #[test]
    fn get_for_host_splits_value_on_tab() {
        seed(&[("split.test", "alice\ttok123")]);
        let got = get_for_host("https://split.test/owner/repo.git").unwrap();
        assert_eq!(got, Some(("alice".to_string(), "tok123".to_string())));
    }

    #[test]
    fn get_for_host_returns_none_without_tab_separator() {
        seed(&[("notab.test", "just-a-token")]);
        assert_eq!(get_for_host("notab.test").unwrap(), None);
    }

    #[test]
    fn resolve_prefers_oauth_token() {
        seed(&[("oauth.test/arbor", "gho_xxx")]);
        let got = resolve_credentials("https://oauth.test/o/r.git").unwrap();
        assert_eq!(got, Some(("x-oauth-basic".to_string(), "gho_xxx".to_string())));
    }

    #[test]
    fn resolve_falls_back_to_host_default() {
        // No OAuth key for this host, only a PAT-style default.
        seed(&[("fallback.test", "bob\tpat456")]);
        let got = resolve_credentials("https://fallback.test/o/r.git").unwrap();
        assert_eq!(got, Some(("bob".to_string(), "pat456".to_string())));
    }

    #[test]
    fn missing_key_reads_none() {
        // Force the vault loaded (empty is fine) so we don't hit the store.
        seed(&[]);
        assert_eq!(get("definitely-absent.test", "").unwrap(), None);
    }
}
