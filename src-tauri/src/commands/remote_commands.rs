//! `open_in_browser` stays a `#[tauri::command]` (OS glue): it drives the system
//! opener through an `AppHandle`, which the router's type-erased `&AppState`
//! context can't carry. The network remote ops (`fetch_remote`, `push_branch`,
//! `pull_branch`) and `list_remotes` moved to the generic router — see
//! [`crate::ipc::corvus::remote`].

use tauri::State;

use crate::error::AppError;
use crate::git::url::{forge_url, normalize_to_https};
use crate::AppState;

// ── Open-in-browser helpers ────────────────────────────────────────────────────
// URL transformations are centralised in git::url to avoid duplication with
// auth::credential_store and pipeline::ci_client.

fn get_first_remote_url(repo: &git2::Repository) -> Result<String, AppError> {
    let remotes = repo.remotes().map_err(AppError::Git)?;
    let name = remotes
        .iter()
        .flatten()
        .find(|r| *r == "origin")
        .or_else(|| remotes.iter().flatten().next())
        .ok_or_else(|| AppError::Other("No remotes configured for this repository".into()))?
        .to_owned();
    let remote = repo.find_remote(&name).map_err(AppError::Git)?;
    remote
        .url()
        .ok_or_else(|| AppError::Other("Remote URL is not valid UTF-8".into()))
        .map(|s| s.to_string())
}

#[tauri::command]
pub fn open_in_browser(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    target: String,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let url = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let remote_url = get_first_remote_url(repo.inner())?;
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
