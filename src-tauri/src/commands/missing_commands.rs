//! Locate flow for projects that are registered in Arbor but no longer present
//! (or reachable) on the filesystem.
//!
//! The leaf classification/probe handlers (`validate_repo_path[s]`,
//! `report_repo_missing`, recent-repo cleanup) and the shared path-status
//! types now live in [`crate::ipc::corvus::missing`]. Only the AppHandle/emit-
//! coupled `relocate_repo` stays inline here, pending a later emit/seam pass.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::error::{AppError, Result};
use crate::ipc::corvus::missing::{classify, RepoPathStatus, RepoPathValidation};
use crate::workspace::registry as registry_io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocateResult {
    pub repo_id:  String,
    pub old_path: String,
    pub new_path: String,
    pub validation: RepoPathValidation,
}

/// Point a registered repo at a new path on disk.  The frontend has already
/// let the user pick the folder; we re-validate (defence-in-depth — the
/// folder could vanish between picker and confirm) and only persist if the
/// destination is a valid git repo.
///
/// Fires `on_project_relocated` so plugins keyed off the absolute path
/// (deps-explorer cache, IDE history, …) can update their bookkeeping.
#[tauri::command]
pub fn relocate_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    repo_id: String,
    new_path: String,
) -> Result<RelocateResult> {
    let validation = classify(&new_path);
    if validation.status != RepoPathStatus::Ok {
        return Err(AppError::Other(format!(
            "Cannot relocate repository: {}",
            validation.message,
        )));
    }

    let old_path = {
        let reg = state.lock_repo_registry()?;
        reg.get(&repo_id)
            .map(|e| e.path.clone())
            .ok_or_else(|| AppError::Other(format!("repo not found: {repo_id}")))?
    };

    // Skip the write-then-read churn when the user picked the same folder.
    if normalize(&old_path) == normalize(&new_path) {
        return Ok(RelocateResult {
            repo_id, old_path: new_path.clone(), new_path, validation,
        });
    }

    let updated = {
        let mut reg = state.lock_repo_registry()?;
        reg.set_path(&repo_id, new_path.clone())?;
        registry_io::save(&reg)?;
        reg.get(&repo_id).cloned()
    };

    // Mirror into the recent_repos list so the WelcomeScreen doesn't keep
    // showing the dead path.  Best-effort; failure here doesn't unwind.
    if let Ok(mut cfg) = state.lock_config() {
        let new_norm = normalize(&new_path);
        let old_norm = normalize(&old_path);
        cfg.recent_repos.retain(|p| normalize(p) != old_norm);
        cfg.recent_repos.retain(|p| normalize(p) != new_norm);
        cfg.recent_repos.insert(0, new_path.clone());
        cfg.recent_repos.truncate(10);
        let _ = crate::config::app_config::save(&cfg);
    }

    if let Some(entry) = updated {
        state.fire_hook("on_project_relocated", serde_json::json!({
            "repo_id":   &repo_id,
            "old_path":  &old_path,
            "new_path":  &new_path,
            "name":      &entry.display_name,
            "remote_url": &entry.remote_url,
        }));
        let _ = app.emit("arbor://repo-relocated", serde_json::json!({
            "repo_id":  &repo_id,
            "old_path": &old_path,
            "new_path": &new_path,
        }));
    }

    Ok(RelocateResult { repo_id, old_path, new_path, validation })
}

fn normalize(p: &str) -> String {
    let s = p.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}
