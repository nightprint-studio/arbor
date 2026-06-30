//! `open_in_browser` stays a `#[tauri::command]` (OS glue): it drives the system
//! opener through an `AppHandle`, which the router's type-erased `&AppState`
//! context can't carry. The network remote ops (`fetch_remote`, `push_branch`,
//! `pull_branch`) and `list_remotes` moved to the generic router — see
//! [`crate::ipc::corvus::remote`].
//!
//! The `origin` URL comes from `corvus-be` (`repo_origin_url` — it owns the repo),
//! so this command runs no git at all: it just builds the forge URL and opens the
//! system browser.

use tauri::State;

use crate::error::AppError;
use crate::git::url::{forge_url, normalize_to_https};
use crate::AppState;

// ── Open-in-browser helpers ────────────────────────────────────────────────────
// URL transformations are centralised in git::url to avoid duplication with
// auth::credential_store and pipeline::ci_client.

#[tauri::command]
pub fn open_in_browser(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    target: String,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let url = {
        let v = crate::ipc::dispatch_rpc(
            state.inner(),
            "corvus",
            "repo_origin_url",
            serde_json::json!({ "tab_id": tab_id }),
        )?;
        let remote_url: Option<String> =
            serde_json::from_value(v).map_err(|e| AppError::Other(e.to_string()))?;
        let remote_url = remote_url.ok_or_else(|| {
            AppError::Other("No 'origin' remote configured for this repository".into())
        })?;
        let base = normalize_to_https(&remote_url).ok_or_else(|| {
            AppError::Other(format!("Cannot build browser URL for remote: {}", remote_url))
        })?;
        forge_url(&base, &target)
    };
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Other(format!("Failed to open browser: {}", e)))?;
    Ok(())
}
