//! `auth` domain — git-host credential store (OS keyring), routed through the
//! in-process broker.
//!
//! Thin sync handlers over [`crate::auth::credential_store`] — username/password
//! (or PAT) entries keyed by host/URL, consumed automatically by fetch/push.
//! They take no real state (the keyring is process-global), so the context is
//! `_state: &AppState` purely to satisfy the handler shape. All sync (keyring
//! I/O is fast), so each registers as `Kind::Sync` and runs on `spawn_blocking`.
//!
//! Lives with the git-auth family (`provider`, `remote`) under the `corvus`
//! program: these are the credentials the git network layer uses.

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

/// Save the "default" credential for a host — used automatically by fetch/push.
#[corvus::handler]
fn save_default_credential(
    _state: &AppState,
    url_or_host: String,
    username: String,
    password: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::save_for_host(&url_or_host, &username, &password)
}

/// Returns true if a default credential is stored for the given host/URL.
#[corvus::handler]
fn has_default_credential(_state: &AppState, url_or_host: String) -> Result<bool, AppError> {
    Ok(crate::auth::credential_store::get_for_host(&url_or_host)?.is_some())
}

/// Delete the default credential for a host/URL.
#[corvus::handler]
fn delete_default_credential(_state: &AppState, url_or_host: String) -> Result<(), AppError> {
    crate::auth::credential_store::delete_for_host(&url_or_host)
}

#[corvus::handler]
fn save_credential(
    _state: &AppState,
    host: String,
    username: String,
    password: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::save(&host, &username, &password)
}

#[corvus::handler]
fn get_credential(
    _state: &AppState,
    host: String,
    username: String,
) -> Result<Option<String>, AppError> {
    crate::auth::credential_store::get(&host, &username)
}

#[corvus::handler]
fn delete_credential(_state: &AppState, host: String, username: String) -> Result<(), AppError> {
    crate::auth::credential_store::delete(&host, &username)
}
