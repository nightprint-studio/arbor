use keyring::Entry;

use crate::error::{AppError, Result};

const SERVICE: &str = "arbor-git-client";

/// Save (or update) a credential in the OS native store.
pub fn save(host: &str, _username: &str, password: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, &format!("{host}"))
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    entry
        .set_password(password)
        .map_err(|e| AppError::AuthFailed(e.to_string()))
}

/// Retrieve a stored credential. Returns `None` if not found.
pub fn get(host: &str, _username: &str) -> Result<Option<String>> {
    let entry = Entry::new(SERVICE, &format!("{host}"))
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

/// Delete a stored credential.
pub fn delete(host: &str, _username: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, &format!("{host}"))
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Host-based default credential (used for automatic fetch/push auth)
//
// Stores a single "default" credential per hostname so that network operations
// can look up credentials without knowing the username in advance.
// Keyring key: "default@{host}" — value: "username\ttoken_or_password"
// ---------------------------------------------------------------------------

/// Save or replace the default credential for a host/URL.
pub fn save_for_host(url_or_host: &str, username: &str, password: &str) -> Result<()> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    let entry = Entry::new(SERVICE, &format!("{host}"))
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    let combined = format!("{username}\t{password}");
    entry
        .set_password(&combined)
        .map_err(|e| AppError::AuthFailed(e.to_string()))
}

/// Retrieve the default (username, password/token) for a host/URL.
pub fn get_for_host(url_or_host: &str) -> Result<Option<(String, String)>> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    let entry = Entry::new(SERVICE, &format!("{host}"))
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.get_password() {
        Ok(s) => {
            if let Some((user, pass)) = s.split_once('\t') {
                Ok(Some((user.to_string(), pass.to_string())))
            } else {
                Ok(None)
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

/// Delete the default credential for a host/URL.
pub fn delete_for_host(url_or_host: &str) -> Result<()> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    let entry = Entry::new(SERVICE, &format!("{host}"))
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
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
    let host = extract_host(url_or_host)
        .unwrap_or_else(|| url_or_host.to_string());

    // 1. OAuth token (Device Flow)
    let oauth_key = format!("{host}/arbor");
    let entry = Entry::new(SERVICE, &oauth_key)
        .map_err(|e| crate::error::AppError::AuthFailed(e.to_string()))?;
    match entry.get_password() {
        Ok(token) => return Ok(Some(("x-oauth-basic".to_string(), token))),
        Err(keyring::Error::NoEntry) => {}
        Err(e) => tracing::warn!("keyring OAuth lookup failed for '{oauth_key}': {e}"),
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
