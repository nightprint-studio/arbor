use tauri::State;

use crate::error::AppError;
use crate::AppState;

// ── Credential store (username + password / PAT) ──────────────────────────

/// Save the "default" credential for a host — used automatically by fetch/push.
#[tauri::command]
pub fn save_default_credential(
    _state: State<'_, AppState>,
    url_or_host: String,
    username: String,
    password: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::save_for_host(&url_or_host, &username, &password)
}

/// Returns true if a default credential is stored for the given host/URL.
#[tauri::command]
pub fn has_default_credential(
    _state: State<'_, AppState>,
    url_or_host: String,
) -> Result<bool, AppError> {
    Ok(crate::auth::credential_store::get_for_host(&url_or_host)?.is_some())
}

/// Delete the default credential for a host/URL.
#[tauri::command]
pub fn delete_default_credential(
    _state: State<'_, AppState>,
    url_or_host: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::delete_for_host(&url_or_host)
}

#[tauri::command]
pub fn save_credential(
    _state: State<'_, AppState>,
    host: String,
    username: String,
    password: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::save(&host, &username, &password)
}

#[tauri::command]
pub fn get_credential(
    _state: State<'_, AppState>,
    host: String,
    username: String,
) -> Result<Option<String>, AppError> {
    crate::auth::credential_store::get(&host, &username)
}

#[tauri::command]
pub fn delete_credential(
    _state: State<'_, AppState>,
    host: String,
    username: String,
) -> Result<(), AppError> {
    crate::auth::credential_store::delete(&host, &username)
}

// OAuth connect / status / disconnect / token-refresh for every provider now
// flow through the generic `{issue,git}_provider_*` commands (see
// `commands::provider_commands` + `provider_connect`). Token refresh happens
// lazily inside each provider's `SessionProvider`/OAuth flow on a 401, so no
// per-provider command wrappers remain here.
