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
//! NOT migrated (stays inline in `missing_commands`, handled by a later
//! emit/seam pass): `relocate_repo` — it takes an `AppHandle`, emits
//! `arbor://repo-relocated`, and fires the `on_project_relocated` hook with an
//! AppHandle-coupled emit on the same path.
//!
//! Hooks: `report_repo_missing` fires `on_project_missing` after it succeeds.
//! Per the generic-path rule the fire moved to
//! [`crate::ipc::corvus::post_hooks`]; the handler returns the resolved
//! display `name` (looked up from the registry) so the post-hook arm can build
//! the payload from the result, with `repo_id`/`path`/`reason` from the params.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::ipc::corvus;
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
    let _ = (&path, &reason); // forwarded into the hook payload from params
    let name = state.lock_repo_registry().ok()
        .and_then(|reg| reg.get(&repo_id).map(|e| e.display_name.clone()));
    Ok(name)
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
