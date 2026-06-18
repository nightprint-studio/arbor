//! `missing` domain — tombstone/locate flow for projects registered in Arbor
//! but no longer present on disk, routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! `validate_repo_path` / `validate_repo_paths` never touched `AppState`, but
//! the handler macro requires a context first arg, so they take `_state:
//! &AppState` and ignore it — the decoded JSON args are unchanged.
//!
//! `relocate_repo` was the last AppHandle/emit-coupled holdout; it now lives
//! here too. Its `arbor://repo-relocated` egress goes through the backend
//! event sink ([`AppState::event_sink`]) instead of an `AppHandle`.
//!
//! Hooks: `report_repo_missing` fires `on_project_missing` after it succeeds.
//! Per the generic-path rule the fire moved to
//! [`crate::ipc::corvus::post_hooks`]; the handler returns the resolved
//! display `name` (looked up from the registry) so the post-hook arm can build
//! the payload from the result, with `repo_id`/`path`/`reason` from the params.
//!
//! `relocate_repo` likewise fires `on_project_relocated` from the post-hook
//! path. The registry entry's `display_name`/`remote_url` aren't in the params,
//! so the handler surfaces them on the result (`name`/`remote_url`, populated
//! only when the relocation actually moved the path); the post-hook arm fires
//! only when `old_path != new_path`, mirroring the original inline gate.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;
use crate::ipc::corvus;
use crate::workspace::registry as registry_io;
use crate::AppState;

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
// Handlers
// ---------------------------------------------------------------------------

/// Lightweight path classifier used by the frontend at startup (per restored
/// tab) and on demand (recent-repo list, retry buttons).  Never opens the
/// repo, so it stays cheap even on a slow network drive (a stat-failure
/// returns immediately).
#[corvus::handler]
fn validate_repo_path(_state: &AppState, path: String) -> Result<RepoPathValidation, AppError> {
    Ok(classify(&path))
}

/// Batch variant — used at startup to classify all snapshot tabs at once.
/// Order of input is preserved in the output.
#[corvus::handler]
fn validate_repo_paths(_state: &AppState, paths: Vec<String>) -> Result<Vec<RepoPathValidation>, AppError> {
    Ok(paths.iter().map(|p| classify(p)).collect())
}

/// Notify the backend that a tab entered the tombstone (missing/unreachable)
/// state.  The frontend already shows the UI — this exists so plugins can
/// react (e.g. cancel an in-flight job that was waiting on the path) and so
/// we have a single place to decay caches.
///
/// The `on_project_missing` hook is fired by the generic post-hook path; this
/// handler returns the resolved display `name` so that arm can build the
/// payload. Returns `None` when the repo isn't in the registry.
#[corvus::handler]
fn report_repo_missing(
    state:   &AppState,
    repo_id: String,
    path:    String,
    reason:  String,
) -> Result<Option<String>, AppError> {
    // Resolve the display name; the registry guard is a temporary in this
    // expression and drops at the `;`, so the lock is released before we fire.
    let name = state.lock_repo_registry().ok()
        .and_then(|reg| reg.get(&repo_id).map(|e| e.display_name.clone()));

    // Fire inline with first-hand data (lock already dropped above). Always
    // fires, matching the post-hook arm; `name` is the resolved Option<String>.
    state.fire_hook("on_project_missing", json!({
        "repo_id": repo_id,
        "path":    path,
        "name":    &name,
        "reason":  reason,
    }));

    Ok(name)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocateResult {
    pub repo_id:  String,
    pub old_path: String,
    pub new_path: String,
    pub validation: RepoPathValidation,
    /// Registry display name of the relocated repo. Populated only when the
    /// relocation actually moved the path (so the `on_project_relocated`
    /// post-hook arm can build its payload); `None` on the same-folder no-op.
    pub name: Option<String>,
    /// Registry remote URL of the relocated repo. Same population rule as
    /// `name` — fed into the post-hook payload from the result.
    pub remote_url: Option<String>,
}

/// Point a registered repo at a new path on disk.  The frontend has already
/// let the user pick the folder; we re-validate (defence-in-depth — the
/// folder could vanish between picker and confirm) and only persist if the
/// destination is a valid git repo.
///
/// The `on_project_relocated` hook is fired by the generic post-hook path
/// (gated on `old_path != new_path`); this handler surfaces the registry
/// entry's `name`/`remote_url` on the result so that arm can build the payload
/// keyed off the absolute path (deps-explorer cache, IDE history, …).
#[corvus::handler]
fn relocate_repo(
    state: &AppState,
    repo_id: String,
    new_path: String,
) -> Result<RelocateResult, AppError> {
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
            repo_id,
            old_path: new_path.clone(),
            new_path,
            validation,
            name: None,
            remote_url: None,
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

    let (name, remote_url) = match &updated {
        Some(entry) => (Some(entry.display_name.clone()), entry.remote_url.clone()),
        None => (None, None),
    };

    // Notify the UI to swap the dead path for the new one. The `on_project_relocated`
    // plugin hook is fired by the generic post-hook path (same `old_path != new_path`
    // gate). Only emit when the registry entry resolved, matching the original inline
    // `if let Some(entry) = updated` guard.
    if updated.is_some() {
        if let Some(sink) = state.event_sink() {
            sink.emit("arbor://repo-relocated", serde_json::json!({
                "repo_id":  &repo_id,
                "old_path": &old_path,
                "new_path": &new_path,
            }));
        }
    }

    // Fire inline with first-hand data; no repo lock is held here. Gated on an
    // actual move — `name` is Some only when the registry entry resolved after
    // `old_path != new_path` (the same-folder no-op returns early above), which
    // mirrors the post-hook arm's `result.name.is_some()` gate.
    if name.is_some() {
        state.fire_hook("on_project_relocated", json!({
            "repo_id":    &repo_id,
            "old_path":   &old_path,
            "new_path":   &new_path,
            "name":       &name,
            "remote_url": &remote_url,
        }));
    }

    Ok(RelocateResult { repo_id, old_path, new_path, validation, name, remote_url })
}

// ---------------------------------------------------------------------------
// Recent-repo cleanup helpers (used by the missing-projects UI)
// ---------------------------------------------------------------------------

/// Remove a path from the recent-repos list.  Path comparison is normalised
/// so `C:\foo` and `C:/foo` match the same entry.
#[corvus::handler]
fn remove_recent_repo(state: &AppState, path: String) -> Result<(), AppError> {
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
#[corvus::handler]
fn cleanup_missing_recent_repos(state: &AppState) -> Result<Vec<String>, AppError> {
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
