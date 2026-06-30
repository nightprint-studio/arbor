//! `workspace` domain — READ-ONLY access to the corvus repo registry + workspaces.
//!
//! The File Explorer's Projects sidebar lists "the user's projects" even when the
//! git client (corvus-be) isn't running. corvus-be OWNS the canonical read+write
//! handlers (mutations are a git-product action); these are read-only twins that
//! parse the same JSON files directly. sitta-be has the active profile
//! (`init_active_profile` ran in `main`), so it composes the corvus product paths
//! itself — no shell push, no shared crate: `repos.json` / `workspaces.json` are
//! plain JSON. The wire shapes match corvus-be's, so the FE decodes either backend.
//!
//! No `SittaState` is touched (the macro requires a context arg, so each handler
//! takes `_state` and ignores it). No hooks fire here.

use arbor_core::prelude::{product_path, PRODUCT_CORVUS};
use serde::{Deserialize, Serialize};

use sitta_core::prelude::SittaState;

const SCRATCH_ID: &str = "scratch";

// ── repos.json ───────────────────────────────────────────────────────────────

/// One registered repository. Mirrors corvus-be's `RepoRegistryEntry` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRegistryEntry {
    pub id:           String,
    pub path:         String,
    #[serde(default)]
    pub remote_url:   Option<String>,
    pub display_name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    entries: Vec<RepoRegistryEntry>,
}

// ── workspaces.json ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGroup {
    pub id:        String,
    pub name:      String,
    #[serde(default)]
    pub order:     u32,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub color_idx: u8,
}

/// A workspace. The reserved extensibility fields (v1-unused) are kept as opaque
/// JSON so the read passes them through byte-identically to corvus-be's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDef {
    pub id:        String,
    pub name:      String,
    #[serde(default)]
    pub color_idx: u8,
    pub repo_ids:  Vec<String>,
    #[serde(default)]
    pub order:     u32,
    #[serde(default)]
    pub group_id:          Option<String>,
    #[serde(default)]
    pub metadata:          serde_json::Value,
    #[serde(default)]
    pub settings_override: Option<serde_json::Value>,
    #[serde(default)]
    pub git_identity:      Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkspaceStoreFile {
    #[serde(default)]
    workspaces:          Vec<WorkspaceDef>,
    #[serde(default)]
    groups:              Vec<WorkspaceGroup>,
    #[serde(default)]
    active_workspace_id: Option<String>,
}

/// `list_workspaces` result — byte-identical to corvus-be's `WorkspacesSnapshot`.
#[derive(Debug, Serialize)]
pub struct WorkspacesSnapshot {
    pub workspaces:          Vec<WorkspaceDef>,
    pub groups:              Vec<WorkspaceGroup>,
    pub active_workspace_id: Option<String>,
}

fn scratch_workspace() -> WorkspaceDef {
    WorkspaceDef {
        id:                SCRATCH_ID.to_string(),
        name:              "Scratch".to_string(),
        color_idx:         0,
        repo_ids:          Vec::new(),
        order:             u32::MAX, // always last
        group_id:          None,
        metadata:          serde_json::Value::Null,
        settings_override: None,
        git_identity:      None,
    }
}

/// Read `workspaces.json` (a missing/corrupt file → defaults), guaranteeing the
/// implicit Scratch workspace + a valid active id, exactly like corvus-be's load.
fn read_store() -> WorkspaceStoreFile {
    let mut store = std::fs::read_to_string(product_path(PRODUCT_CORVUS, "workspaces.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<WorkspaceStoreFile>(&s).ok())
        .unwrap_or_default();
    if !store.workspaces.iter().any(|w| w.id == SCRATCH_ID) {
        store.workspaces.push(scratch_workspace());
    }
    if store.active_workspace_id.is_none() {
        store.active_workspace_id = Some(SCRATCH_ID.to_string());
    }
    store
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// Workspaces + groups + the active id, read straight from `workspaces.json`.
/// Read-only — workspace mutations stay a git-product (corvus) action.
#[arbor_rpc::handler]
fn list_workspaces(_state: &SittaState) -> Result<WorkspacesSnapshot, String> {
    let store = read_store();
    let mut workspaces = store.workspaces;
    workspaces.sort_by(|a, b| (a.order, a.name.to_lowercase()).cmp(&(b.order, b.name.to_lowercase())));
    let mut groups = store.groups;
    groups.sort_by(|a, b| (a.order, a.name.to_lowercase()).cmp(&(b.order, b.name.to_lowercase())));
    Ok(WorkspacesSnapshot { workspaces, groups, active_workspace_id: store.active_workspace_id })
}

/// Registered repos, read straight from `repos.json`, sorted by display name.
#[arbor_rpc::handler]
fn list_registry_repos(_state: &SittaState) -> Result<Vec<RepoRegistryEntry>, String> {
    let mut entries = std::fs::read_to_string(product_path(PRODUCT_CORVUS, "repos.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<RegistryFile>(&s).ok())
        .unwrap_or_default()
        .entries;
    entries.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    Ok(entries)
}
