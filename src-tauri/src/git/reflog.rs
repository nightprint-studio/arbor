//! `reflog` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The reflog projection moved into [`corvus_git::reflog`] (so the headless
//! `corvus-be` shares it). This module keeps the original `crate::git::reflog::*`
//! API. Since `crate::git::repo::GitRepo` now *is* the crate's `GitRepo`, the
//! `&GitRepo` argument forwards straight through with no adaptation.

use crate::error::Result;
use crate::git::repo::GitRepo;

// Re-export the DTO so existing `crate::git::reflog::ReflogEntry` paths resolve.
pub use corvus_git::prelude::ReflogEntry;

pub fn get_reflog(repo: &GitRepo, limit: Option<usize>) -> Result<Vec<ReflogEntry>> {
    Ok(corvus_git::reflog::get_reflog(repo, limit)?)
}
