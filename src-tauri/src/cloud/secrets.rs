//! Thin keyring wrapper for cloud-storage secrets.
//!
//! Two kinds of strings live here:
//!   - Inline service-account JSON (key `gcs/<config-id>`).
//!   - Google OAuth refresh-token blob (key `gcs/<config-id>/oauth`, value is
//!     a JSON `{ refresh_token, token, expires_at, ... }` serialised by
//!     `oauth_google.rs`).
//!
//! Both kinds are opaque to this module — the caller knows what string it
//! put in and how to read it back out.
//!
//! Keys are namespaced under a dedicated keyring service so they cannot
//! collide with `auth/credential_store.rs` entries.

use keyring::Entry;

use crate::error::{AppError, Result};

const SERVICE: &str = "arbor-cloud-storage";

/// Set (or overwrite) a secret.
pub fn set(secret_ref: &str, value: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, secret_ref)
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    entry.set_password(value)
        .map_err(|e| AppError::AuthFailed(e.to_string()))
}

/// Retrieve a secret. Returns `None` if missing — never an error.
pub fn get(secret_ref: &str) -> Result<Option<String>> {
    let entry = Entry::new(SERVICE, secret_ref)
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

/// Delete a secret. Missing is a successful no-op.
pub fn delete(secret_ref: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, secret_ref)
        .map_err(|e| AppError::AuthFailed(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

/// Test that a secret exists without exposing its value.
pub fn exists(secret_ref: &str) -> Result<bool> {
    Ok(get(secret_ref)?.is_some())
}
