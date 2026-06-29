//! `open_in_browser` stays a `#[tauri::command]` (OS glue): it drives the system
//! opener through an `AppHandle`, which the router's type-erased `&AppState`
//! context can't carry. The network remote ops (`fetch_remote`, `push_branch`,
//! `pull_branch`) and `list_remotes` moved to the generic router — see
//! [`crate::ipc::corvus::remote`].
//!
//! The repo path and `origin` URL come from the launcher's git-free path —
//! `corvus-be` resolves the `tab_id` (it owns the open-tab registry) and the
//! `origin` URL is read through the git CLI ([`crate::git::url::probe_origin_url`]),
//! so this command needs neither `git2` nor the shell's old `RepoManager`.

use tauri::State;

use crate::error::AppError;
use crate::git::url::{forge_url, normalize_to_https, probe_origin_url};
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
        let path = crate::ipc::resolve_tab_path(state.inner(), &tab_id)?;
        let remote_url = probe_origin_url(std::path::Path::new(&path)).ok_or_else(|| {
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
