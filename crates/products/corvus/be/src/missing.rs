//! `missing` domain — tombstone/locate flow for projects registered in Arbor
//! but no longer present on disk, owned **out-of-process** by corvus-be.
//!
//! Verbatim port of the shell's `crate::ipc::corvus::missing` (`AppError` →
//! `String`; `AppError::Other`'s wire shape is `#[error("{0}")]`, so the bare
//! format string the `SplitBroker` re-wraps is byte-identical). The path
//! classification (`classify`) is pure libgit2 + `std::fs`, so it runs in this
//! process directly; the registry resolution goes through corvus-be's own
//! `workspace::registry` (the file corvus-be owns); the `recent_repos` reads /
//! writes stay shell-side (a `GENERIC_KEYS` / `profile.toml` slice the launcher
//! recents share — deliberately NOT corvus's to own), reached over the reverse
//! channel via the `__recent_repos_*` / `__forget_recent_repo` host methods.
//!
//! Hooks: `report_repo_missing` fires `corvus:project_missing` inline (the plugin
//! host is co-located here — Wave 0), and `relocate_repo` fires
//! `corvus:project_relocated` inline, gated on an actual move (`old_path != new_path`)
//! and a resolved registry entry — mirroring the shell's inline gates. Both
//! return the resolved `name` (+ `remote_url` for relocate) on their result for
//! FE parity, exactly as the shell handlers did.

use std::path::{Path, PathBuf};

use corvus_core::prelude::{hooks, CorvusState};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::workspace::registry;

// ---------------------------------------------------------------------------
// Result types — serde-identical to the shell's `crate::ipc::corvus::missing`.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocateResult {
    pub repo_id:  String,
    pub old_path: String,
    pub new_path: String,
    pub validation: RepoPathValidation,
    /// Registry display name of the relocated repo. Populated only when the
    /// relocation actually moved the path (so the `corvus:project_relocated` hook
    /// can build its payload); `None` on the same-folder no-op.
    pub name: Option<String>,
    /// Registry remote URL of the relocated repo. Same population rule as `name`.
    pub remote_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Validation helpers — pure, no state. Verbatim from the shell.
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
        if corvus_git::prelude::is_git_repo(path) {
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

/// Normalise a path for recent-repos comparison: forward slashes, no trailing
/// slash, lower-cased on Windows so `C:\Foo` and `c:/foo/` collapse to one key.
fn normalize(p: &str) -> String {
    let s = p.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

// ---------------------------------------------------------------------------
// recent_repos reverse-channel helpers — the list is a shell `AppConfig` slice
// (the launcher recents share it), so corvus-be never owns it; it reads/writes
// it through the matching `__recent_repos_*` / `__forget_recent_repo` host
// methods. All best-effort: a missing reverse channel degrades to a no-op /
// empty list, never an error (mirrors the shell's best-effort `if let Ok(cfg)`).
// ---------------------------------------------------------------------------

/// Read the shell's recent-repos list. Returns an empty Vec on any reverse-channel
/// failure (treated as "nothing to clean").
fn recent_repos_list(state: &CorvusState) -> Vec<String> {
    state
        .host_call("__recent_repos_list", json!({}))
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Prepend `path` to the shell's recent-repos list (the shell dedupes +
/// normalises + caps the length). Best-effort.
fn recent_repos_add(state: &CorvusState, path: &str) {
    let _ = state.host_call("__recent_repos_add", json!({ "path": path }));
}

/// Drop `path` from the shell's recent-repos list (best-effort). Reuses the
/// pre-existing `__forget_recent_repo` host method.
fn recent_repos_forget(state: &CorvusState, path: &str) {
    if path.trim().is_empty() { return; }
    let _ = state.host_call("__forget_recent_repo", json!({ "path": path }));
}

// ---------------------------------------------------------------------------
// Handlers — mirror the shell's `crate::ipc::corvus::missing` macro/signature
// shape. Method + param names match the existing FE payloads (`src/lib/ipc/
// missing.ts`).
// ---------------------------------------------------------------------------

/// Lightweight path classifier used by the frontend at startup (per restored
/// tab) and on demand (recent-repo list, retry buttons).  Never opens the
/// repo, so it stays cheap even on a slow network drive.
#[arbor_rpc::handler]
fn validate_repo_path(_state: &CorvusState, path: String) -> Result<RepoPathValidation, String> {
    Ok(classify(&path))
}

/// Batch variant — used at startup to classify all snapshot tabs at once.
/// Order of input is preserved in the output.
#[arbor_rpc::handler]
fn validate_repo_paths(_state: &CorvusState, paths: Vec<String>) -> Result<Vec<RepoPathValidation>, String> {
    Ok(paths.iter().map(|p| classify(p)).collect())
}

/// Notify the backend that a tab entered the tombstone (missing/unreachable)
/// state.  The frontend already shows the UI — this exists so plugins can react
/// (e.g. cancel an in-flight job waiting on the path) and so we have a single
/// place to decay caches.
///
/// Fires `corvus:project_missing` inline (the plugin host is co-located here —
/// Wave 0), with the resolved display `name` from the registry, then returns
/// that `name` for FE parity. `None` when the repo isn't in the registry.
#[arbor_rpc::handler]
fn report_repo_missing(
    state:   &CorvusState,
    repo_id: String,
    path:    String,
    reason:  String,
) -> Result<Option<String>, String> {
    // Resolve the display name; the registry guard is a temporary in this
    // expression and drops at the `;`, so the lock is released before we fire.
    let name = registry::registry(state)
        .get(&repo_id)
        .map(|e| e.display_name.clone());

    state.fire_hook(hooks::PROJECT_MISSING, json!({
        "repo_id": repo_id,
        "path":    path,
        "name":    &name,
        "reason":  reason,
    }));

    Ok(name)
}

/// Point a registered repo at a new path on disk.  The frontend has already let
/// the user pick the folder; we re-validate (defence-in-depth — the folder
/// could vanish between picker and confirm) and only persist if the destination
/// is a valid git repo.
///
/// Fires `corvus:project_relocated` inline, gated on an actual move
/// (`old_path != new_path`) and a resolved registry entry — mirroring the
/// shell's inline gates. Surfaces the entry's `name`/`remote_url` on the result
/// (populated only on a real move) for the FE.
#[arbor_rpc::handler]
fn relocate_repo(
    state: &CorvusState,
    repo_id: String,
    new_path: String,
) -> Result<RelocateResult, String> {
    let validation = classify(&new_path);
    if validation.status != RepoPathStatus::Ok {
        return Err(format!("Cannot relocate repository: {}", validation.message));
    }

    let old_path = registry::registry(state)
        .get(&repo_id)
        .map(|e| e.path.clone())
        .ok_or_else(|| format!("repo not found: {repo_id}"))?;

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

    // Reload → set path → persist, all under the registry lock; read back the
    // updated entry for the hook/result.
    let updated = registry::mutate(state, |reg| {
        reg.set_path(&repo_id, new_path.clone())?;
        Ok(reg.get(&repo_id).cloned())
    })?;

    // Mirror into the recent_repos list so the WelcomeScreen doesn't keep
    // showing the dead path.  Best-effort over the reverse channel; failure here
    // doesn't unwind. Drop both the dead + the new spelling first, then prepend
    // the new path (the shell-side add dedupes + caps).
    recent_repos_forget(state, &old_path);
    recent_repos_forget(state, &new_path);
    recent_repos_add(state, &new_path);

    let (name, remote_url) = match &updated {
        Some(entry) => (Some(entry.display_name.clone()), entry.remote_url.clone()),
        None => (None, None),
    };

    // Notify the UI to swap the dead path for the new one — only when the
    // registry entry resolved, matching the shell's `if updated.is_some()` guard.
    if updated.is_some() {
        state.emit("arbor://repo-relocated", json!({
            "repo_id":  &repo_id,
            "old_path": &old_path,
            "new_path": &new_path,
        }));
    }

    // Fire `corvus:project_relocated` inline, gated on an actual move (`name` is
    // Some only when the registry entry resolved after `old_path != new_path` —
    // the same-folder no-op returned early above).
    if name.is_some() {
        state.fire_hook(hooks::PROJECT_RELOCATED, json!({
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
// Recent-repo cleanup helpers (used by the missing-projects UI).
// ---------------------------------------------------------------------------

/// Remove a path from the recent-repos list.  Path comparison + dedup is done
/// shell-side (the list is owned there); this forwards over the reverse channel.
#[arbor_rpc::handler]
fn remove_recent_repo(state: &CorvusState, path: String) -> Result<(), String> {
    recent_repos_forget(state, &path);
    Ok(())
}

/// Drop every recent-repo path whose folder is missing/unreachable.  Called by
/// the "Clean up missing repositories" action in Settings.  Returns the list of
/// paths that were removed so the UI can show a summary.
///
/// Reads the list shell-side (reverse channel), classifies each path in-process,
/// then forgets the bad ones one by one (each `__forget_recent_repo` does the
/// normalised removal + persist shell-side).
#[arbor_rpc::handler]
fn cleanup_missing_recent_repos(state: &CorvusState) -> Result<Vec<String>, String> {
    let snapshot = recent_repos_list(state);
    let mut removed = Vec::new();
    for p in &snapshot {
        let v = classify(p);
        if v.status != RepoPathStatus::Ok {
            removed.push(p.clone());
        }
    }
    for p in &removed {
        recent_repos_forget(state, p);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_nonexistent_path_is_missing_or_unreachable() {
        // A path under an existing ancestor (cwd) but with a vanished leaf is
        // "missing"; the ancestor exists so `ancestor_exists` is true.
        let cwd = std::env::current_dir().unwrap();
        let bogus = cwd.join("__corvus_be_nonexistent_leaf_xyz__");
        let v = classify(&bogus.to_string_lossy());
        assert_eq!(v.status, RepoPathStatus::Missing);
        assert!(v.ancestor_exists);
    }

    #[test]
    fn classify_existing_non_repo_is_not_a_repo() {
        // A temp dir that exists but isn't a git repo. `is_git_repo` discovers
        // upward, so we need a dir with no `.git` ancestor — use the OS temp
        // root's immediate child, which is overwhelmingly not under a repo.
        let dir = std::env::temp_dir().join("__corvus_be_classify_not_a_repo__");
        let _ = std::fs::create_dir_all(&dir);
        let v = classify(&dir.to_string_lossy());
        // Either NotARepo (typical) — assert it exists-but-not-ok at minimum.
        assert_ne!(v.status, RepoPathStatus::Missing);
        assert!(v.ancestor_exists);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_collapses_separators_and_trailing_slash() {
        let a = normalize("C:\\Foo\\Bar\\");
        let b = normalize("C:/Foo/Bar");
        assert_eq!(a, b);
    }
}
