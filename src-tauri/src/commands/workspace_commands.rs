//! Recent-repos GC helper — the shell-side remnant of the workspace domain.
//!
//! The workspace command handlers + DTOs (and the orphan-GC, which now fires from
//! corvus-be's own `close_repo`) all moved out-of-process to corvus-be (ADR-1:
//! each backend owns its own `repo_registry` + `workspaces`). What stays here is
//! the one helper the shell still needs in-process:
//!
//! - [`forget_recent_repo`] — answers corvus-be's `__forget_recent_repo` host
//!   call (the `recent_repos` list is a shell `AppConfig` slice).

use crate::error::Result;
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

