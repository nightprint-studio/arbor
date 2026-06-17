//! `search` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The commit-search revwalk moved into [`corvus_git::search`] (so the headless
//! `corvus-be` shares it). This module keeps the original shell-facing API —
//! same `crate::git::search::{SearchQuery, SearchResult, search_commits}` paths,
//! `AppError`-flavored `Result` — so existing consumers are untouched. The logic
//! is pure git2 (no CLI shell-out), so there is nothing to inject.

use git2::Repository;

use crate::error::Result;

// Re-export the data types so existing `crate::git::search::*` paths resolve.
pub use corvus_git::search::{SearchQuery, SearchResult};

pub fn search_commits(repo: &Repository, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    Ok(corvus_git::search::search_commits(repo, query)?)
}
