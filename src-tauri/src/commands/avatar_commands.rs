//! Avatar resolution exposed to the frontend.
//!
//! The commit-graph avatar layer calls `resolve_avatar_for_email` for
//! each visible author email; the backend asks the repo's GitProvider
//! (GitHub / GitLab) for that user's `avatar_url`. Everything is best-
//! effort: any failure (no remote, no token, no match) returns `None`
//! and the frontend falls back to a generated initials avatar.

use tauri::State;

use crate::AppState;
use crate::error::Result;
use crate::git_provider::{avatar_lookup, helpers::provider_for_tab};

#[tauri::command]
pub async fn resolve_avatar_for_email(
    state:  State<'_, AppState>,
    tab_id: String,
    email:  String,
) -> Result<Option<String>> {
    // No provider for this repo (local-only, Bitbucket, …) → quietly None.
    let resolved = match provider_for_tab(&state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(None),
    };
    Ok(avatar_lookup::resolve_for(&resolved, &email).await)
}
