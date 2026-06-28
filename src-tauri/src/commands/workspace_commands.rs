//! Registry-orphan GC helpers — the shell-side remnant of the workspace domain.
//!
//! The workspace command handlers + DTOs all moved out-of-process to corvus-be
//! (ADR-1: each backend owns its own `repo_registry` + `workspaces`). What stays
//! here are the two GC helpers the shell still needs in-process:
//!
//! - [`forget_recent_repo`] — answers corvus-be's `__forget_recent_repo` host
//!   call (the `recent_repos` list is a shell `AppConfig` slice).
//! - [`forget_repo_if_orphaned`] — called by `repo::close_repo` when an orphan's
//!   last tab closes, so "forget an orphan" stays in one place. It reads/writes
//!   the repo registry + workspace store through the shell's reload-on-access
//!   accessors (`lock_repo_registry` / `lock_workspaces`), which back the same
//!   files corvus-be owns — so the shell and corvus-be never drift.

use crate::error::Result;
use crate::workspace::registry as registry_io;
use crate::AppState;

/// Normalise a path for recent-repos comparison: forward slashes, no trailing
/// slash, lower-cased on Windows so `C:\Foo` and `c:/foo/` collapse to one key.
fn norm_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// Drop a path from the recent-repos list (best-effort, no-op when absent or
/// when the repo has no on-disk path). Centralised so every "forget a repo"
/// path cleans the same surface.
pub(crate) fn forget_recent_repo(state: &AppState, path: &str) -> Result<()> {
    if path.trim().is_empty() { return Ok(()); }
    let mut cfg = state.lock_config()?;
    let target = norm_path(path);
    let before = cfg.recent_repos.len();
    cfg.recent_repos.retain(|p| norm_path(p) != target);
    if cfg.recent_repos.len() != before {
        let _ = crate::config::app_config::save(&cfg);
    }
    Ok(())
}

/// "Forget" a repo once it's no longer a member of any workspace.
///
/// When the user removes a repo from its last workspace — or deletes the
/// workspace that held it — Arbor drops the registry entry and its recent-repos
/// pointer, so a later import no longer matches it as "use existing". The folder
/// on disk is never touched: this is purely about Arbor forgetting it.
///
/// Guards:
/// - still referenced by another workspace → not an orphan, left alone.
/// - currently open in a tab → kept (a tab whose repo vanished from the registry
///   would break); it'll be cleaned up the normal way when the tab is closed.
///
/// Fires `on_repo_deregistered` (so plugins drop per-repo caches) and returns
/// `true` when the entry was actually removed. The caller must hold no locks.
///
/// Note: corvus-be has its own twin of this (`crate::workspace::forget_repo_if_orphaned`
/// over there) for the workspace-mutation handlers it now owns; this copy is the
/// shell's, for `close_repo`. Both read/write the same files (reload-on-access),
/// so they stay consistent.
pub(crate) fn forget_repo_if_orphaned(
    state: &AppState,
    repo_id: &str,
    reason: &str,
) -> Result<bool> {
    // Still a member somewhere? Then it's not an orphan.
    if state.lock_workspaces()?.repo_is_in_any_workspace(repo_id) {
        return Ok(false);
    }
    // Need path + name for the recent-repos cleanup and the hook payload.
    let entry = {
        let reg = state.lock_repo_registry()?;
        reg.get(repo_id).map(|e| (e.path.clone(), e.display_name.clone()))
    };
    let Some((path, name)) = entry else { return Ok(false); };
    // Don't yank a repo out from under an open tab.
    let in_open_tab = state.lock_repos()
        .map(|mgr| mgr.all_info().iter().any(|i| i.path == path))
        .unwrap_or(false);
    if in_open_tab { return Ok(false); }
    // Drop the registry entry.
    {
        let mut reg = state.lock_repo_registry()?;
        reg.remove(repo_id);
        registry_io::save(&reg)?;
    }
    // Drop the recent-repos pointer too.
    let _ = forget_recent_repo(state, &path);
    state.fire_hook("on_repo_deregistered", serde_json::json!({
        "repo_id": repo_id,
        "path":    path,
        "name":    name,
        "reason":  reason,
    }));
    Ok(true)
}
