//! `stats` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The revwalk aggregation moved into [`corvus_git::stats`] (so the headless
//! `corvus-be` shares it). This module keeps the original shell-facing API —
//! same `compute_stats` signature, `AppError` results — so the in-process
//! consumers (the stats broker handler, the stats cache type) are untouched.
//!
//! The only coupling the crate refuses is the per-repo config type: the walk
//! takes a crate-owned [`corvus_git::stats::StatsExclude`] value-struct, and
//! this wrapper adapts the shell's `StatsExcludeConfig` into it. `compute_stats`
//! is pure `git2` — no `GitCli` and no recovery snapshot to inject.

use git2::Repository;

use crate::config::repo_config::StatsExcludeConfig;
use crate::error::Result;

// Re-export the data types so existing `crate::git::stats::*` paths resolve.
pub use corvus_git::stats::RepoStats;

/// Adapt the shell's serde config into the crate's value-struct (no serde,
/// no config coupling). Behaviour-preserving: same three field lists.
fn to_exclude(cfg: &StatsExcludeConfig) -> corvus_git::stats::StatsExclude {
    corvus_git::stats::StatsExclude {
        extensions: cfg.extensions.clone(),
        folders:    cfg.folders.clone(),
        files:      cfg.files.clone(),
    }
}

pub fn compute_stats(repo: &Repository, exclude: &StatsExcludeConfig) -> Result<RepoStats> {
    Ok(corvus_git::stats::compute_stats(repo, &to_exclude(exclude))?)
}
