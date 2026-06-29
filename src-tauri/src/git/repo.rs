//! `repo` — shell-local repo metadata DTO.
//!
//! The git2 `RepoManager` cache is gone from the launcher: `corvus-be` owns the
//! open-tab → path registry and produces all git-derived metadata (current
//! branch, bare/empty flags) through `get_repo_info`. The shell keeps only this
//! small serde DTO so the in-process `open_repo` handler can return the same JSON
//! shape the frontend expects — deserialized from `corvus-be`'s response. Field
//! names/types mirror `corvus_git`'s `RepoInfo` exactly so the round-trip is
//! byte-identical.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub tab_id: String,
    pub path: String,
    pub name: String,
    pub current_branch: Option<String>,
    pub is_bare: bool,
    pub is_empty: bool,
}
