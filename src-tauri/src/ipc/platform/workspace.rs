//! `workspace` domain (platform) — READ-ONLY access to the repo registry +
//! workspaces, served by the always-on platform backend.
//!
//! `repos.json` + `workspaces.json` are plain JSON files, but they describe "the
//! user's projects" — an app-level concept other products want to *read* even when
//! the git backend isn't running. corvus-be owns the canonical read+WRITE handlers
//! (mutations are a git-product action); these platform twins are read-only mirrors
//! so an optional consumer (the File Explorer's Projects sidebar) can list projects
//! without spawning corvus-be. Same files, same shapes — the file is the single
//! source of truth and both sides reload on access, so the two never drift.

use serde::Serialize;

use crate::error::AppError;
use crate::ipc::platform;
use crate::workspace::registry::{self, RepoRegistryEntry};
use crate::workspace::store::{self, WorkspaceDef, WorkspaceGroup};
use crate::AppState;

/// Snapshot returned by [`list_workspaces`] — byte-identical shape to corvus-be's
/// `WorkspacesSnapshot`, so the FE `WorkspacesSnapshot` type is satisfied by either
/// backend.
#[derive(Debug, Serialize)]
struct WorkspacesSnapshot {
    workspaces: Vec<WorkspaceDef>,
    groups: Vec<WorkspaceGroup>,
    active_workspace_id: Option<String>,
}

/// List workspaces + groups + the active id, read straight from `workspaces.json`.
/// Read-only: workspace mutations stay a git-product (corvus) action.
#[platform::handler(program = "platform")]
fn list_workspaces(_state: &AppState) -> Result<WorkspacesSnapshot, AppError> {
    let s = store::load();
    let mut groups = s.groups.clone();
    groups.sort_by(|a, b| {
        (a.order, a.name.to_lowercase()).cmp(&(b.order, b.name.to_lowercase()))
    });
    Ok(WorkspacesSnapshot {
        workspaces: s.ordered(),
        groups,
        active_workspace_id: s.active_workspace_id.clone(),
    })
}

/// List registered repos, read straight from `repos.json`. Read-only.
#[platform::handler(program = "platform")]
fn list_registry_repos(_state: &AppState) -> Result<Vec<RepoRegistryEntry>, AppError> {
    Ok(registry::load().list())
}
