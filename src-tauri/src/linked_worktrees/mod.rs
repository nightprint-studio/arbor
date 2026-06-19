// ---------------------------------------------------------------------------
// Linked worktrees — **shell-side residue** after the full-move to `corvus-be`.
//
// The live registry, the 13 CRUD handlers, the cross-repo checkout-sync
// orchestrator and the branch worktree-link handlers all moved out-of-process
// (`crates/corvus/be/src/{linked_worktree.rs, worktree_links/}`). corvus-be owns
// the registry and persists it to `linked_worktrees.toml`.
//
// What remains here is the minimum the **shell** still needs:
//  - [`links_file_path`] — the profile-aware path the shell pushes to corvus-be
//    (a separate process that can't compute it) via the `worktree_links_path`
//    config section;
//  - the registry types + [`load`] / [`save`] — used by the `arbor.linked_worktrees`
//    plugin namespace (`plugin/ns_shell/linked_worktrees.rs`), which reads/writes
//    the same file (the single source of truth shared with corvus-be).
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

// ── Types (serde-identical to the corvus-be copy) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeLink {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub sync_enabled: bool,
    #[serde(default)]
    pub members: Vec<LinkMember>,
    #[serde(default)]
    pub alias_groups: Vec<AliasGroup>,
    #[serde(default)]
    pub last_sync_target: Option<SyncTarget>,
    #[serde(default)]
    pub created_at: i64,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkMember {
    pub repo_id: String,
    #[serde(default = "default_true")]
    pub sync_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasGroup {
    pub id: String,
    pub members: Vec<AliasEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AliasEntry {
    pub repo_id: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTarget {
    pub initiator_repo_id: String,
    pub branch: String,
    pub timestamp: i64,
}

// ── Minimal registry (the plugin namespace's read + sync-toggle surface) ──────

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct LinksFile {
    #[serde(default)]
    links: Vec<WorktreeLink>,
}

#[derive(Debug, Default, Clone)]
pub struct WorktreeLinkRegistry {
    links: HashMap<String, WorktreeLink>,
}

impl WorktreeLinkRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn list(&self) -> Vec<WorktreeLink> {
        let mut v: Vec<_> = self.links.values().cloned().collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    }

    pub fn get(&self, id: &str) -> Option<&WorktreeLink> {
        self.links.get(id)
    }

    pub fn set_sync_enabled(&mut self, link_id: &str, enabled: bool) -> Result<()> {
        let l = self
            .links
            .get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        l.sync_enabled = enabled;
        Ok(())
    }

    fn replace_all(&mut self, list: Vec<WorktreeLink>) {
        self.links.clear();
        for l in list {
            self.links.insert(l.id.clone(), l);
        }
    }
}

// ── Persistence (the shared `linked_worktrees.toml`) ──────────────────────────

pub fn links_file_path() -> PathBuf {
    arbor_core::prelude::product_path(arbor_core::prelude::PRODUCT_CORVUS, "linked_worktrees.toml")
}

pub fn load() -> WorktreeLinkRegistry {
    let path = links_file_path();
    if !path.exists() {
        return WorktreeLinkRegistry::new();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return WorktreeLinkRegistry::new();
    };
    let Ok(file) = toml::from_str::<LinksFile>(&content) else {
        return WorktreeLinkRegistry::new();
    };
    let mut reg = WorktreeLinkRegistry::new();
    reg.replace_all(file.links);
    reg
}

pub fn save(reg: &WorktreeLinkRegistry) -> Result<()> {
    let path = links_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut links: Vec<_> = reg.links.values().cloned().collect();
    links.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let content = toml::to_string_pretty(&LinksFile { links })
        .map_err(|e| AppError::Other(format!("linked_worktrees: serialize failed: {e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}
