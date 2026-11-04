//! Hirundo shell glue — the values of `@secret` variables.
//!
//! Like Picus's connection password, a Hirundo secret goes from the form **straight to the shell's keychain**
//! and never enters the backend: `hirundo-be` asks for it over the reverse channel (`__hirundo_secret`) at
//! the moment a request is built. The account text comes from the backend's `secret_account` handler (which
//! knows project, layer and variable name); the `hirundo/` namespace is applied here by
//! [`hirundo_secret_account`], the same function the reverse-channel door uses, so both have identical rules.

use crate::error::{AppError, Result};
use crate::ipc::hirundo_secret_account;

/// Store (or replace) a secret variable's value. An empty value deletes the entry.
#[tauri::command]
pub fn hirundo_store_secret(account: String, secret: String) -> Result<()> {
    let account = hirundo_secret_account(&account).map_err(AppError::Other)?;
    if secret.is_empty() {
        return crate::auth::credential_store::delete(&account, "");
    }
    crate::auth::credential_store::save(&account, "", &secret)
}

#[tauri::command]
pub fn hirundo_delete_secret(account: String) -> Result<()> {
    let account = hirundo_secret_account(&account).map_err(AppError::Other)?;
    crate::auth::credential_store::delete(&account, "")
}

/// Whether a value is stored — a boolean, never the value, so the form can say "saved" without showing it.
#[tauri::command]
pub fn hirundo_has_secret(account: String) -> Result<bool> {
    let account = hirundo_secret_account(&account).map_err(AppError::Other)?;
    Ok(crate::auth::credential_store::get(&account, "")?.is_some_and(|s| !s.is_empty()))
}
