//! Picus shell glue — the connection password.
//!
//! One narrow job, and it is here rather than in `picus-be` on purpose: **the
//! password never reaches the backend process at all**. The connection form sends
//! it straight to the shell, the shell puts it in Arbor's keychain, and `picus-be`
//! later asks for it over the reverse channel at the moment it opens a session
//! (`__picus_secret`). One hop fewer, and the secret exists in exactly one process
//! that has any business holding it.
//!
//! The keychain account is namespaced by [`picus_secret_account`], which also
//! validates the id — the same function the reverse-channel handler uses, so both
//! doors have identical rules.

use crate::error::{AppError, Result};
use crate::ipc::picus_secret_account;

/// Store (or replace) the password for a Picus connection.
///
/// An empty secret **deletes** the entry rather than storing a blank one: clearing
/// the field in the form means "this connection has no password", and leaving an
/// empty string behind would make `has_secret` lie.
#[tauri::command]
pub fn picus_store_secret(connection_id: String, secret: String) -> Result<()> {
    let account = picus_secret_account(&connection_id).map_err(AppError::Other)?;
    if secret.is_empty() {
        return crate::auth::credential_store::delete(&account, "");
    }
    crate::auth::credential_store::save(&account, "", &secret)
}

/// Forget a connection's password.
///
/// Also called by `picus-be` over the reverse channel when a connection is deleted,
/// so removing a connection never leaves an orphaned password in the keychain.
#[tauri::command]
pub fn picus_delete_secret(connection_id: String) -> Result<()> {
    let account = picus_secret_account(&connection_id).map_err(AppError::Other)?;
    crate::auth::credential_store::delete(&account, "")
}

/// Whether a password is stored for this connection.
///
/// Returns a boolean, never the value: the form needs to say "a password is saved"
/// instead of showing an empty field that looks like data loss, and that is all it
/// needs to know.
#[tauri::command]
pub fn picus_has_secret(connection_id: String) -> Result<bool> {
    let account = picus_secret_account(&connection_id).map_err(AppError::Other)?;
    Ok(crate::auth::credential_store::get(&account, "")?.is_some_and(|s| !s.is_empty()))
}
