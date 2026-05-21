// ---------------------------------------------------------------------------
// Linked Worktrees — cross-project sync of branch checkouts.
//
// A "worktree link" groups several worktrees (each identified by their
// RepoRegistry UUID — keyed by path).  When the user checks out a branch on
// a member, the action is propagated to every other member of the link
// (with optional per-link branch aliases).
//
// Persistence: ~/.config/arbor/linked_worktrees.toml.  Identity of a member
// is the repo_id from RepoRegistry — survives renames of the display name
// or relocations through the registry.
//
// V1 supports only the "checkout" operation; the orchestrator is structured
// around a `LinkOperation` enum so future ops (drop worktree, pull, …) can
// reuse the same plumbing.
// ---------------------------------------------------------------------------

pub mod aliases;
pub mod orchestrator;

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeLink {
    pub id:               String,
    pub name:             String,
    #[serde(default = "default_true")]
    pub sync_enabled:     bool,
    #[serde(default)]
    pub members:          Vec<LinkMember>,
    #[serde(default)]
    pub alias_groups:     Vec<AliasGroup>,
    #[serde(default)]
    pub last_sync_target: Option<SyncTarget>,
    #[serde(default)]
    pub created_at:       i64,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkMember {
    pub repo_id: String,
    /// Per-member sync toggle.  When `false`, the orchestrator never
    /// propagates checkouts to this member nor accepts checkouts from it as
    /// triggers.  `#[serde(default = "default_true")]` so existing snapshots
    /// (saved before this field existed) load with sync ON.
    #[serde(default = "default_true")]
    pub sync_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasGroup {
    pub id:      String,
    pub members: Vec<AliasEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AliasEntry {
    pub repo_id: String,
    pub branch:  String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTarget {
    pub initiator_repo_id: String,
    pub branch:            String,
    pub timestamp:         i64,
}

// ---------------------------------------------------------------------------
// Sync results — emitted to frontend + plugin hooks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemberStatus {
    Updated   { branch: String },
    SkippedMissing { branch: String },
    Conflict  { branch: String, files: Vec<String> },
    Error     { message: String },
    Skipped   { reason: String },           // e.g. "broken member", "same branch already"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberResult {
    pub repo_id: String,
    pub status:  MemberStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSummary {
    pub link_id:           String,
    pub link_name:         String,
    pub target_branch:     String,
    pub initiator_repo_id: String,
    pub results:           Vec<MemberResult>,
}

// ---------------------------------------------------------------------------
// Operations (extensible — V1 only Checkout)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum LinkOperation {
    Checkout { #[allow(dead_code)] branch: String },
    // Future:
    // DropWorktree,
    // Pull,
    // Fetch,
}

// ---------------------------------------------------------------------------
// Registry — in-memory, persisted to linked_worktrees.toml
// ---------------------------------------------------------------------------

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

    pub fn get_mut(&mut self, id: &str) -> Option<&mut WorktreeLink> {
        self.links.get_mut(id)
    }

    /// Find the link containing a given repo_id (a repo is in at most one).
    pub fn find_by_repo(&self, repo_id: &str) -> Option<&WorktreeLink> {
        self.links.values().find(|l| l.members.iter().any(|m| m.repo_id == repo_id))
    }

    pub fn find_by_repo_mut(&mut self, repo_id: &str) -> Option<&mut WorktreeLink> {
        self.links.values_mut().find(|l| l.members.iter().any(|m| m.repo_id == repo_id))
    }

    pub fn create(&mut self, name: String, initial_repo_ids: Vec<String>) -> Result<WorktreeLink> {
        // Validate members aren't in another link.
        for rid in &initial_repo_ids {
            if let Some(existing) = self.find_by_repo(rid) {
                return Err(AppError::Other(format!(
                    "repo {} is already a member of link '{}'", rid, existing.name
                )));
            }
        }
        let id = Uuid::new_v4().to_string();
        let link = WorktreeLink {
            id: id.clone(),
            name,
            sync_enabled: true,
            members: initial_repo_ids.into_iter().map(|rid| LinkMember { repo_id: rid, sync_enabled: true }).collect(),
            alias_groups: vec![],
            last_sync_target: None,
            created_at: chrono::Utc::now().timestamp(),
        };
        let cloned = link.clone();
        self.links.insert(id, link);
        Ok(cloned)
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        self.links.remove(id)
            .ok_or_else(|| AppError::Other(format!("link not found: {id}")))?;
        Ok(())
    }

    pub fn rename(&mut self, id: &str, name: String) -> Result<()> {
        let l = self.links.get_mut(id)
            .ok_or_else(|| AppError::Other(format!("link not found: {id}")))?;
        l.name = name;
        Ok(())
    }

    pub fn add_member(&mut self, link_id: &str, repo_id: &str) -> Result<()> {
        // Verify repo isn't in another link.
        if let Some(other) = self.find_by_repo(repo_id) {
            if other.id != link_id {
                return Err(AppError::Other(format!(
                    "repo is already in link '{}'", other.name
                )));
            }
        }
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        if !l.members.iter().any(|m| m.repo_id == repo_id) {
            l.members.push(LinkMember { repo_id: repo_id.to_string(), sync_enabled: true });
        }
        Ok(())
    }

    pub fn remove_member(&mut self, link_id: &str, repo_id: &str) -> Result<()> {
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        l.members.retain(|m| m.repo_id != repo_id);
        // Also clean any alias entries referencing this repo.
        for g in l.alias_groups.iter_mut() {
            g.members.retain(|e| e.repo_id != repo_id);
        }
        // Drop alias groups that fell below 2 members.
        l.alias_groups.retain(|g| g.members.len() >= 2);
        Ok(())
    }

    pub fn set_sync_enabled(&mut self, link_id: &str, enabled: bool) -> Result<()> {
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        l.sync_enabled = enabled;
        Ok(())
    }

    /// Toggle sync for a single member of a link.  When `enabled=false`, the
    /// orchestrator skips this member entirely — it never receives propagated
    /// checkouts, and checkouts originating from it don't trigger the sync.
    pub fn set_member_sync_enabled(&mut self, link_id: &str, repo_id: &str, enabled: bool) -> Result<()> {
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        let m = l.members.iter_mut().find(|m| m.repo_id == repo_id)
            .ok_or_else(|| AppError::Other(format!("member {repo_id} not in link {link_id}")))?;
        m.sync_enabled = enabled;
        Ok(())
    }

    /// Add an alias group.  Validates: ≥2 members, no `(repo_id, branch)`
    /// already present in another alias group of the same link.
    pub fn add_alias_group(&mut self, link_id: &str, members: Vec<AliasEntry>) -> Result<AliasGroup> {
        if members.len() < 2 {
            return Err(AppError::Other("alias group needs at least 2 members".into()));
        }
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        for e in &members {
            if !l.members.iter().any(|m| m.repo_id == e.repo_id) {
                return Err(AppError::Other(format!(
                    "alias entry references non-member repo {}", e.repo_id
                )));
            }
        }
        for e in &members {
            for g in &l.alias_groups {
                if g.members.iter().any(|x| x.repo_id == e.repo_id && x.branch == e.branch) {
                    return Err(AppError::Other(format!(
                        "({}, {}) is already in another alias group", e.repo_id, e.branch
                    )));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        for e in &members {
            let k = format!("{}::{}", e.repo_id, e.branch);
            if !seen.insert(k) {
                return Err(AppError::Other(format!(
                    "duplicate entry ({}, {}) in alias group", e.repo_id, e.branch
                )));
            }
        }
        let group = AliasGroup { id: Uuid::new_v4().to_string(), members };
        l.alias_groups.push(group.clone());
        Ok(group)
    }

    pub fn update_alias_group(&mut self, link_id: &str, group_id: &str, members: Vec<AliasEntry>) -> Result<()> {
        if members.len() < 2 {
            return Err(AppError::Other("alias group needs at least 2 members".into()));
        }
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        for e in &members {
            if !l.members.iter().any(|m| m.repo_id == e.repo_id) {
                return Err(AppError::Other(format!(
                    "alias entry references non-member repo {}", e.repo_id
                )));
            }
        }
        for e in &members {
            for g in l.alias_groups.iter().filter(|g| g.id != group_id) {
                if g.members.iter().any(|x| x.repo_id == e.repo_id && x.branch == e.branch) {
                    return Err(AppError::Other(format!(
                        "({}, {}) is already in another alias group", e.repo_id, e.branch
                    )));
                }
            }
        }
        let g = l.alias_groups.iter_mut().find(|g| g.id == group_id)
            .ok_or_else(|| AppError::Other(format!("alias group not found: {group_id}")))?;
        g.members = members;
        Ok(())
    }

    pub fn remove_alias_group(&mut self, link_id: &str, group_id: &str) -> Result<()> {
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        let before = l.alias_groups.len();
        l.alias_groups.retain(|g| g.id != group_id);
        if l.alias_groups.len() == before {
            return Err(AppError::Other(format!("alias group not found: {group_id}")));
        }
        Ok(())
    }

    pub fn set_sync_target(&mut self, link_id: &str, t: SyncTarget) -> Result<()> {
        let l = self.links.get_mut(link_id)
            .ok_or_else(|| AppError::Other(format!("link not found: {link_id}")))?;
        l.last_sync_target = Some(t);
        Ok(())
    }

    /// Replace contents (used on reload).
    pub fn replace_all(&mut self, list: Vec<WorktreeLink>) {
        self.links.clear();
        for l in list { self.links.insert(l.id.clone(), l); }
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn links_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("linked_worktrees.toml")
}

pub fn load() -> WorktreeLinkRegistry {
    let path = links_file_path();
    if !path.exists() { return WorktreeLinkRegistry::new(); }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("linked_worktrees: read failed, starting empty: {e}");
            return WorktreeLinkRegistry::new();
        }
    };
    let file: LinksFile = match toml::from_str(&content) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("linked_worktrees: parse failed, starting empty: {e}");
            return WorktreeLinkRegistry::new();
        }
    };
    let mut reg = WorktreeLinkRegistry::new();
    reg.replace_all(file.links);
    reg
}

pub fn save(reg: &WorktreeLinkRegistry) -> Result<()> {
    let path = links_file_path();
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let mut links: Vec<_> = reg.links.values().cloned().collect();
    links.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let file = LinksFile { links };
    let content = toml::to_string_pretty(&file)
        .map_err(|e| AppError::Other(format!("linked_worktrees: serialize failed: {e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}
