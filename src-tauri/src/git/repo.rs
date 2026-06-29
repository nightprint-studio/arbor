//! `repo` — shell-side re-export of the `corvus-git` repo types.
//!
//! The repository handle, metadata DTOs, and the in-memory open-repo registry
//! (`RepoManager`) live in [`corvus_git::repo`] (shared with the headless
//! `corvus-be`); the types are re-exported here so existing `crate::git::repo::*`
//! paths (`RepoInfo`, `RepoManager`) keep resolving — `app_state` and the corvus
//! repo IPC handler still reach them through this module. The clone /
//! remote-listing helpers and the background clone job moved fully into
//! `corvus-be` (driven there by the per-product plugin host).

// Re-export the open-repo registry + repo DTO so existing `crate::git::repo::*`
// paths resolve unchanged.
pub use corvus_git::prelude::{RepoInfo, RepoManager};
