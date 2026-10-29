//! Assistant shell glue — the provider key, in its own vault.
//!
//! Same shape and same reason as the Picus password: the key goes from the Settings page straight
//! to the shell and into the OS store, and **never reaches a backend process until the moment the
//! assistant makes a request** (it is then asked for over the reverse channel). The key lives in the
//! assistant's own vault — see `credential_store::Vault` — so a long key cannot crowd out the git
//! tokens in the default one.

use crate::auth::credential_store;
use crate::error::{AppError, Result};

/// Providers are named by a short lowercase id (`anthropic`); anything else is refused so a typo
/// can never create a second, unreachable entry.
fn provider_id(provider: &str) -> Result<&str> {
    let ok = !provider.is_empty()
        && provider.len() <= 32
        && provider.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok { Ok(provider) } else { Err(AppError::Other(format!("invalid assistant provider id `{provider}`"))) }
}

/// Store (or replace) the key for a provider. A blank key **clears** it: emptying the field means
/// "no key", and a stored blank would make `assistant_has_key` lie.
#[tauri::command]
pub fn assistant_store_key(provider: String, key: String) -> Result<()> {
    let provider = provider_id(&provider)?;
    if key.trim().is_empty() {
        return credential_store::delete_assistant_key(provider);
    }
    credential_store::save_assistant_key(provider, &key)
}

/// Forget the key for a provider.
#[tauri::command]
pub fn assistant_delete_key(provider: String) -> Result<()> {
    credential_store::delete_assistant_key(provider_id(&provider)?)
}

/// Whether a key is stored — a boolean, never the value, so the page can say "a key is saved"
/// instead of showing an empty field that looks like data loss.
#[tauri::command]
pub fn assistant_has_key(provider: String) -> Result<bool> {
    Ok(credential_store::get_assistant_key(provider_id(&provider)?)?.is_some_and(|k| !k.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_are_short_lowercase_words() {
        assert!(provider_id("anthropic").is_ok());
        assert!(provider_id("open-router2").is_ok());
        assert!(provider_id("").is_err());
        assert!(provider_id("Anthropic").is_err());
        assert!(provider_id("a/b").is_err());
        assert!(provider_id(&"x".repeat(33)).is_err());
    }
}
