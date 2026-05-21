//! Tombstone + locate flow for projects that are registered in Arbor but no
//! longer present (or reachable) on the filesystem.
//!
//! Three categories are reported back to the UI so it can pick distinct UX:
//!  - `missing`     — path doesn't exist, but at least one ancestor does;
//!                    the directory was almost certainly deleted/moved.
//!  - `unreachable` — neither the path nor any of its ancestors can be
//!                    stat-ed; typical on a network share that's offline or
//!                    a removable drive that isn't mounted.
//!  - `not_a_repo`  — the path exists but is no longer a git repository
//!                    (e.g. `.git/` was wiped while the metadata stayed).
//!  - `ok`          — path exists and is a valid git repo.
//!
//! See also: `on_project_missing` / `on_project_relocated` plugin hooks.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::error::{AppError, Result};
use crate::workspace::registry as registry_io;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepoPathStatus {
    Ok,
    Missing,
    Unreachable,
    NotARepo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoPathValidation {
    pub status:  RepoPathStatus,
    /// Human-readable explanation, suitable for display in the tombstone UI.
    pub message: String,
    /// True when at least one ancestor of `path` exists on disk.  Used by the
    /// caller to distinguish "deleted folder" from "drive offline".
    pub ancestor_exists: bool,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Walk parents until we find one that `Path::exists()` succeeds on.  If we
/// exhaust the chain, the whole prefix is unreachable (drive unmounted,
/// network share offline, …).
fn ancestor_exists(p: &Path) -> bool {
    let mut cur: Option<&Path> = p.parent();
    while let Some(parent) = cur {
        if parent.as_os_str().is_empty() { break; }
        if parent.exists() { return true; }
        cur = parent.parent();
    }
    false
}

/// Synchronous path classification.  Cheap — does at most a handful of
/// `metadata()` calls plus a `Repository::discover()` if the path exists.
pub fn classify(path: &str) -> RepoPathValidation {
    let p = PathBuf::from(path);

    if p.exists() {
        if crate::git::init::is_git_repo(path) {
            return RepoPathValidation {
                status:  RepoPathStatus::Ok,
                message: String::new(),
                ancestor_exists: true,
            };
        }
        return RepoPathValidation {
            status:  RepoPathStatus::NotARepo,
            message: "The folder exists but no longer contains a git repository.".into(),
            ancestor_exists: true,
        };
    }

    let anc = ancestor_exists(&p);
    if anc {
        RepoPathValidation {
            status:  RepoPathStatus::Missing,
            message: "The folder no longer exists on disk.".into(),
            ancestor_exists: true,
        }
    } else {
        RepoPathValidation {
            status:  RepoPathStatus::Unreachable,
            message: "The drive or network share is currently unavailable.".into(),
            ancestor_exists: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Lightweight path classifier used by the frontend at startup (per restored
/// tab) and on demand (recent-repo list, retry buttons).  Never opens the
/// repo, so it stays cheap even on a slow network drive (a stat-failure
/// returns immediately).
#[tauri::command]
pub fn validate_repo_path(path: String) -> Result<RepoPathValidation> {
    Ok(classify(&path))
}

/// Batch variant — used at startup to classify all snapshot tabs at once.
/// Order of input is preserved in the output.
#[tauri::command]
pub fn validate_repo_paths(paths: Vec<String>) -> Result<Vec<RepoPathValidation>> {
    Ok(paths.iter().map(|p| classify(p)).collect())
}

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
        if let Ok(host) = state.lock_plugin_host() {
            let _ = host.fire_hook("on_project_relocated", &serde_json::json!({
                "repo_id":   &repo_id,
                "old_path":  &old_path,
                "new_path":  &new_path,
                "name":      &entry.display_name,
                "remote_url": &entry.remote_url,
            }).to_string());
        }
        let _ = app.emit("arbor://repo-relocated", serde_json::json!({
            "repo_id":  &repo_id,
            "old_path": &old_path,
            "new_path": &new_path,
        }));
    }

    Ok(RelocateResult { repo_id, old_path, new_path, validation })
}

/// Notify the backend that a tab entered the tombstone (missing/unreachable)
/// state.  The frontend already shows the UI — this exists so plugins can
/// react (e.g. cancel an in-flight job that was waiting on the path) and so
/// we have a single place to decay caches.  No-op when no plugins are
/// listening.
#[tauri::command]
pub fn report_repo_missing(
    state: State<'_, AppState>,
    repo_id: String,
    path: String,
    reason: String,
) -> Result<()> {
    let name = state.lock_repo_registry().ok()
        .and_then(|reg| reg.get(&repo_id).map(|e| e.display_name.clone()));

    if let Ok(host) = state.lock_plugin_host() {
        let _ = host.fire_hook("on_project_missing", &serde_json::json!({
            "repo_id": &repo_id,
            "path":    &path,
            "name":    &name,
            "reason":  &reason,
        }).to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Recent-repo cleanup helpers (used by the missing-projects UI)
// ---------------------------------------------------------------------------

/// Remove a path from the recent-repos list.  Path comparison is normalised
/// so `C:\foo` and `C:/foo` match the same entry.
#[tauri::command]
pub fn remove_recent_repo(state: State<'_, AppState>, path: String) -> Result<()> {
    let target = normalize(&path);
    let mut cfg = state.lock_config()?;
    let before = cfg.recent_repos.len();
    cfg.recent_repos.retain(|p| normalize(p) != target);
    if cfg.recent_repos.len() != before {
        crate::config::app_config::save(&cfg).map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}

/// Drop every recent-repo path whose folder is missing/unreachable.  Called
/// by the "Clean up missing repositories" action in Settings.  Returns the
/// list of paths that were removed so the UI can show a summary.
#[tauri::command]
pub fn cleanup_missing_recent_repos(state: State<'_, AppState>) -> Result<Vec<String>> {
    let snapshot = {
        let cfg = state.lock_config()?;
        cfg.recent_repos.clone()
    };
    let mut removed = Vec::new();
    for p in &snapshot {
        let v = classify(p);
        if v.status != RepoPathStatus::Ok {
            removed.push(p.clone());
        }
    }
    if !removed.is_empty() {
        let mut cfg = state.lock_config()?;
        let removed_norm: Vec<String> = removed.iter().map(|p| normalize(p)).collect();
        cfg.recent_repos.retain(|p| !removed_norm.contains(&normalize(p)));
        crate::config::app_config::save(&cfg).map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(removed)
}

fn normalize(p: &str) -> String {
    let s = p.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}
