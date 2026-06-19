//! Worktree-link registry + types — the cross-repo branch-checkout-sync model,
//! owned **out-of-process** by corvus-be (the full-move cutover).
//!
//! Ported from the shell's `crate::linked_worktrees` (`AppError` → `String`; the
//! `AppError::Other` wire shape is `#[error("{0}")]`, so the bare format string
//! the `SplitBroker` re-wraps is byte-identical). The registry is **process-local
//! module state** ([`REGISTRY`]), lazily (re)loaded from the
//! `linked_worktrees.toml` path the shell pushes through the `worktree_links_path`
//! config section: corvus-be is a separate process and cannot compute the
//! profile-aware path itself, so the shell (which owns the active profile) hands
//! it over. A path change (profile switch) triggers a reload on the next access.
//! Writes go through [`mutate`], which saves under the registry lock — the same
//! save-timing the shell used (`save(&reg)`).
//!
//! NOTE: this lands Phase 1+2 of the full-move (the registry + the 13 CRUD
//! handlers in `be/src/linked_worktree.rs`). The orchestrator (Phase 3) and the
//! branch worktree-link handlers (Phase 4) consume the alias helpers, the
//! sync-result types and `set_sync_target` — currently unused here — hence the
//! scoped `allow(dead_code)` until those phases land.
#![allow(dead_code)]

pub mod aliases;
pub mod orchestrator;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex, MutexGuard};

use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Types (serde-identical to the shell's `crate::linked_worktrees`) ──────────

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemberStatus {
    Updated { branch: String },
    SkippedMissing { branch: String },
    Conflict { branch: String, files: Vec<String> },
    Error { message: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberResult {
    pub repo_id: String,
    pub status: MemberStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSummary {
    pub link_id: String,
    pub link_name: String,
    pub target_branch: String,
    pub initiator_repo_id: String,
    pub results: Vec<MemberResult>,
}

/// Extensible op set (V1 only `Checkout`); kept for the orchestrator (Phase 3).
#[derive(Debug, Clone)]
pub enum LinkOperation {
    Checkout { branch: String },
}

/// Unix-seconds timestamp — matches the shell's `chrono::Utc::now().timestamp()`
/// without pulling `chrono` into this crate.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Registry ──────────────────────────────────────────────────────────────────

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

    pub fn create(&mut self, name: String, initial_repo_ids: Vec<String>) -> Result<WorktreeLink, String> {
        for rid in &initial_repo_ids {
            if let Some(existing) = self.find_by_repo(rid) {
                return Err(format!("repo {} is already a member of link '{}'", rid, existing.name));
            }
        }
        let id = Uuid::new_v4().to_string();
        let link = WorktreeLink {
            id: id.clone(),
            name,
            sync_enabled: true,
            members: initial_repo_ids
                .into_iter()
                .map(|rid| LinkMember { repo_id: rid, sync_enabled: true })
                .collect(),
            alias_groups: vec![],
            last_sync_target: None,
            created_at: now_secs(),
        };
        let cloned = link.clone();
        self.links.insert(id, link);
        Ok(cloned)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.links.remove(id).ok_or_else(|| format!("link not found: {id}"))?;
        Ok(())
    }

    pub fn rename(&mut self, id: &str, name: String) -> Result<(), String> {
        let l = self.links.get_mut(id).ok_or_else(|| format!("link not found: {id}"))?;
        l.name = name;
        Ok(())
    }

    pub fn add_member(&mut self, link_id: &str, repo_id: &str) -> Result<(), String> {
        if let Some(other) = self.find_by_repo(repo_id) {
            if other.id != link_id {
                return Err(format!("repo is already in link '{}'", other.name));
            }
        }
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        if !l.members.iter().any(|m| m.repo_id == repo_id) {
            l.members.push(LinkMember { repo_id: repo_id.to_string(), sync_enabled: true });
        }
        Ok(())
    }

    pub fn remove_member(&mut self, link_id: &str, repo_id: &str) -> Result<(), String> {
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        l.members.retain(|m| m.repo_id != repo_id);
        for g in l.alias_groups.iter_mut() {
            g.members.retain(|e| e.repo_id != repo_id);
        }
        l.alias_groups.retain(|g| g.members.len() >= 2);
        Ok(())
    }

    pub fn set_sync_enabled(&mut self, link_id: &str, enabled: bool) -> Result<(), String> {
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        l.sync_enabled = enabled;
        Ok(())
    }

    pub fn set_member_sync_enabled(&mut self, link_id: &str, repo_id: &str, enabled: bool) -> Result<(), String> {
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        let m = l
            .members
            .iter_mut()
            .find(|m| m.repo_id == repo_id)
            .ok_or_else(|| format!("member {repo_id} not in link {link_id}"))?;
        m.sync_enabled = enabled;
        Ok(())
    }

    pub fn add_alias_group(&mut self, link_id: &str, members: Vec<AliasEntry>) -> Result<AliasGroup, String> {
        if members.len() < 2 {
            return Err("alias group needs at least 2 members".into());
        }
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        for e in &members {
            if !l.members.iter().any(|m| m.repo_id == e.repo_id) {
                return Err(format!("alias entry references non-member repo {}", e.repo_id));
            }
        }
        for e in &members {
            for g in &l.alias_groups {
                if g.members.iter().any(|x| x.repo_id == e.repo_id && x.branch == e.branch) {
                    return Err(format!("({}, {}) is already in another alias group", e.repo_id, e.branch));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        for e in &members {
            let k = format!("{}::{}", e.repo_id, e.branch);
            if !seen.insert(k) {
                return Err(format!("duplicate entry ({}, {}) in alias group", e.repo_id, e.branch));
            }
        }
        let group = AliasGroup { id: Uuid::new_v4().to_string(), members };
        l.alias_groups.push(group.clone());
        Ok(group)
    }

    pub fn update_alias_group(&mut self, link_id: &str, group_id: &str, members: Vec<AliasEntry>) -> Result<(), String> {
        if members.len() < 2 {
            return Err("alias group needs at least 2 members".into());
        }
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        for e in &members {
            if !l.members.iter().any(|m| m.repo_id == e.repo_id) {
                return Err(format!("alias entry references non-member repo {}", e.repo_id));
            }
        }
        for e in &members {
            for g in l.alias_groups.iter().filter(|g| g.id != group_id) {
                if g.members.iter().any(|x| x.repo_id == e.repo_id && x.branch == e.branch) {
                    return Err(format!("({}, {}) is already in another alias group", e.repo_id, e.branch));
                }
            }
        }
        let g = l
            .alias_groups
            .iter_mut()
            .find(|g| g.id == group_id)
            .ok_or_else(|| format!("alias group not found: {group_id}"))?;
        g.members = members;
        Ok(())
    }

    pub fn remove_alias_group(&mut self, link_id: &str, group_id: &str) -> Result<(), String> {
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        let before = l.alias_groups.len();
        l.alias_groups.retain(|g| g.id != group_id);
        if l.alias_groups.len() == before {
            return Err(format!("alias group not found: {group_id}"));
        }
        Ok(())
    }

    pub fn set_sync_target(&mut self, link_id: &str, t: SyncTarget) -> Result<(), String> {
        let l = self.links.get_mut(link_id).ok_or_else(|| format!("link not found: {link_id}"))?;
        l.last_sync_target = Some(t);
        Ok(())
    }

    /// Replace contents (used on reload).
    pub fn replace_all(&mut self, list: Vec<WorktreeLink>) {
        self.links.clear();
        for l in list {
            self.links.insert(l.id.clone(), l);
        }
    }

    /// Snapshot every link (for the orchestrator's alias resolution, Phase 3).
    pub fn all(&self) -> Vec<WorktreeLink> {
        self.links.values().cloned().collect()
    }
}

// ── Process-local registry + path-driven (re)load ─────────────────────────────

static REGISTRY: LazyLock<Mutex<WorktreeLinkRegistry>> = LazyLock::new(Default::default);
static LOADED_PATH: Mutex<Option<String>> = Mutex::new(None);

fn links_path(state: &CorvusState) -> Option<String> {
    state
        .config("worktree_links_path")
        .and_then(|v| v.as_str().map(String::from))
}

fn load_from(path: &Path) -> WorktreeLinkRegistry {
    let mut reg = WorktreeLinkRegistry::new();
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(file) = toml::from_str::<LinksFile>(&content) {
            reg.replace_all(file.links);
        }
    }
    reg
}

/// Reload the registry when the shell-pushed path differs from the loaded one
/// (first access loads; a profile switch re-pushes a new path → reload).
fn ensure_loaded(state: &CorvusState) {
    let want = links_path(state);
    let mut loaded = LOADED_PATH.lock().unwrap_or_else(|p| p.into_inner());
    if *loaded != want {
        let reg = match want.as_deref() {
            Some(p) => load_from(Path::new(p)),
            None => WorktreeLinkRegistry::new(),
        };
        *REGISTRY.lock().unwrap_or_else(|p| p.into_inner()) = reg;
        *loaded = want;
    }
}

/// Read-access to the (lazily loaded) registry.
pub fn registry(state: &CorvusState) -> MutexGuard<'static, WorktreeLinkRegistry> {
    ensure_loaded(state);
    REGISTRY.lock().unwrap_or_else(|p| p.into_inner())
}

/// Mutate the registry and persist under the lock — the same save-timing the
/// shell used (`reg.op(); save(&reg)`). Returns the closure's value.
pub fn mutate<T>(
    state: &CorvusState,
    f: impl FnOnce(&mut WorktreeLinkRegistry) -> Result<T, String>,
) -> Result<T, String> {
    ensure_loaded(state);
    let mut reg = REGISTRY.lock().unwrap_or_else(|p| p.into_inner());
    let result = f(&mut reg)?;
    save_to(&reg, &links_path(state))?;
    Ok(result)
}

/// Persist the link's `last_sync_target` after an orchestrator run — locks the
/// live registry and saves to the captured path. Used by the orchestrator
/// thread, which holds no `&CorvusState`.
pub fn commit_sync_target(link_id: &str, target: SyncTarget, path: &Option<String>) {
    let mut reg = REGISTRY.lock().unwrap_or_else(|p| p.into_inner());
    let _ = reg.set_sync_target(link_id, target);
    let _ = save_to(&reg, path);
}

fn save_to(reg: &WorktreeLinkRegistry, path: &Option<String>) -> Result<(), String> {
    let Some(path) = path else {
        // No path pushed yet — nothing to persist to (should not happen once the
        // shell has synced; the in-memory mutation still stands for this session).
        return Ok(());
    };
    let mut links: Vec<_> = reg.links.values().cloned().collect();
    links.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let content = toml::to_string_pretty(&LinksFile { links })
        .map_err(|e| format!("linked_worktrees: serialize failed: {e}"))?;
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}
