//! Workspace store (`workspaces.json`) — owned **out-of-process** by corvus-be.
//!
//! Ported from the shell's `crate::workspace::store` (`AppError` → `String`). The
//! **file is the single source of truth** (see [`registry`](super::registry) for
//! the dual-writer rationale): every access reloads it; writes go through
//! [`mutate`]. The shell pushes the profile-aware path via the `workspaces_path`
//! config section.

use std::path::Path;
use std::sync::{LazyLock, Mutex, MutexGuard};

use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Fixed id for the implicit "Scratch" workspace. Non-deletable, non-renameable.
pub const SCRATCH_ID: &str = "scratch";

/// Reserved for a future per-workspace git identity override (unused v1, baked
/// into the schema so adding it later needs no migration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitIdentity {
    pub name:  String,
    pub email: String,
}

/// Optional visual parent for one or more workspaces (pure UI organisation aid).
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

/// A workspace — ordered group of repo ids plus some cosmetic data.
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
    // ── Reserved extensibility fields (unused v1, persisted). ────────────────
    #[serde(default)]
    pub metadata:          serde_json::Value,
    #[serde(default)]
    pub settings_override: Option<serde_json::Value>,
    #[serde(default)]
    pub git_identity:      Option<GitIdentity>,
}

/// On-disk shape of `workspaces.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStore {
    #[serde(default)]
    pub workspaces:          Vec<WorkspaceDef>,
    #[serde(default)]
    pub groups:              Vec<WorkspaceGroup>,
    #[serde(default)]
    pub active_workspace_id: Option<String>,
}

impl Default for WorkspaceStore {
    fn default() -> Self {
        Self {
            workspaces:          vec![Self::new_scratch()],
            groups:              Vec::new(),
            active_workspace_id: Some(SCRATCH_ID.to_string()),
        }
    }
}

impl WorkspaceStore {
    pub fn new_scratch() -> WorkspaceDef {
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

    pub fn ensure_scratch(&mut self) {
        if !self.workspaces.iter().any(|w| w.id == SCRATCH_ID) {
            self.workspaces.push(Self::new_scratch());
        }
    }

    pub fn get(&self, id: &str) -> Option<&WorkspaceDef> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut WorkspaceDef> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    pub fn active(&self) -> Option<&WorkspaceDef> {
        let id = self.active_workspace_id.as_ref()?;
        self.get(id)
    }

    /// Find a workspace by case-insensitive name within a group scope.
    pub fn find_by_name_in_group(&self, name: &str, group_id: Option<&str>) -> Option<&WorkspaceDef> {
        let name = name.trim();
        self.workspaces.iter().find(|w|
            w.name.trim().eq_ignore_ascii_case(name) && w.group_id.as_deref() == group_id
        )
    }

    /// Union `repo_ids` into an existing workspace (idempotent merge).
    pub fn merge_repos_into(&mut self, workspace_id: &str, repo_ids: &[String]) -> Result<(), String> {
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        for id in repo_ids {
            if !ws.repo_ids.iter().any(|x| x == id) {
                ws.repo_ids.push(id.clone());
            }
        }
        Ok(())
    }

    pub fn create(
        &mut self,
        name: String,
        color_idx: u8,
        repo_ids: Vec<String>,
        group_id: Option<String>,
    ) -> WorkspaceDef {
        let next_order = self.workspaces.iter()
            .filter(|w| w.id != SCRATCH_ID)
            .map(|w| w.order)
            .max()
            .map(|o| o.saturating_add(1))
            .unwrap_or(0);
        let ws = WorkspaceDef {
            id:                Uuid::new_v4().to_string(),
            name,
            color_idx,
            repo_ids,
            order:             next_order,
            group_id:          group_id.filter(|id| !id.is_empty()),
            metadata:          serde_json::Value::Null,
            settings_override: None,
            git_identity:      None,
        };
        self.workspaces.push(ws.clone());
        ws
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        if id == SCRATCH_ID {
            return Err("cannot delete the Scratch workspace".into());
        }
        self.workspaces.retain(|w| w.id != id);
        if self.active_workspace_id.as_deref() == Some(id) {
            self.active_workspace_id = Some(SCRATCH_ID.to_string());
        }
        Ok(())
    }

    pub fn set_order(&mut self, ordered_ids: &[String]) {
        for (i, id) in ordered_ids.iter().enumerate() {
            if id == SCRATCH_ID { continue; } // Scratch stays pinned bottom
            if let Some(w) = self.get_mut(id) { w.order = i as u32; }
        }
    }

    pub fn add_repo(&mut self, workspace_id: &str, repo_id: &str) -> Result<(), String> {
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        if !ws.repo_ids.iter().any(|id| id == repo_id) {
            ws.repo_ids.push(repo_id.to_string());
        }
        Ok(())
    }

    pub fn remove_repo(&mut self, workspace_id: &str, repo_id: &str) -> Result<(), String> {
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        ws.repo_ids.retain(|id| id != repo_id);
        Ok(())
    }

    /// Fully purge a repo id from every workspace.
    pub fn purge_repo_everywhere(&mut self, repo_id: &str) {
        for ws in &mut self.workspaces {
            ws.repo_ids.retain(|id| id != repo_id);
        }
    }

    /// True when at least one workspace (including Scratch) lists `repo_id`.
    pub fn repo_is_in_any_workspace(&self, repo_id: &str) -> bool {
        self.workspaces.iter().any(|ws| ws.repo_ids.iter().any(|id| id == repo_id))
    }

    /// Workspaces sorted by `order` (Scratch always last because order = u32::MAX).
    pub fn ordered(&self) -> Vec<WorkspaceDef> {
        let mut v = self.workspaces.clone();
        v.sort_by_key(|w| (w.order, w.name.to_lowercase()));
        v
    }

    // ── Group management ──────────────────────────────────────────────────────

    pub fn create_group(&mut self, name: String, color_idx: u8) -> WorkspaceGroup {
        let next_order = self.groups.iter().map(|g| g.order).max()
            .map(|o| o.saturating_add(1))
            .unwrap_or(0);
        let g = WorkspaceGroup {
            id: Uuid::new_v4().to_string(),
            name,
            order: next_order,
            collapsed: false,
            color_idx,
        };
        self.groups.push(g.clone());
        g
    }

    pub fn get_group(&self, id: &str) -> Option<&WorkspaceGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn get_group_mut(&mut self, id: &str) -> Option<&mut WorkspaceGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    pub fn remove_group(&mut self, id: &str) -> Result<(), String> {
        // Orphan children — they reappear at top level, no cascade-delete.
        for ws in &mut self.workspaces {
            if ws.group_id.as_deref() == Some(id) { ws.group_id = None; }
        }
        self.groups.retain(|g| g.id != id);
        Ok(())
    }

    pub fn set_group_order(&mut self, ordered_ids: &[String]) {
        for (i, id) in ordered_ids.iter().enumerate() {
            if let Some(g) = self.get_group_mut(id) { g.order = i as u32; }
        }
    }

    pub fn set_workspace_group(&mut self, workspace_id: &str, group_id: Option<String>) -> Result<(), String> {
        if workspace_id == SCRATCH_ID {
            return Err("Scratch cannot be placed inside a group".into());
        }
        let resolved = group_id.filter(|id| !id.is_empty())
            .and_then(|id| if self.get_group(&id).is_some() { Some(id) } else { None });
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        ws.group_id = resolved;
        Ok(())
    }
}

// ── Persistence — the file is the single source of truth ──────────────────────

static STORE: LazyLock<Mutex<WorkspaceStore>> = LazyLock::new(|| Mutex::new(WorkspaceStore::default()));

fn store_file_path(state: &CorvusState) -> Option<String> {
    state
        .config("workspaces_path")
        .and_then(|v| v.as_str().map(String::from))
}

/// Load + normalise (ensure scratch, valid active id) — mirrors the shell's
/// `store::load`. A missing/unreadable file yields the default store.
fn load_from(path: &Path) -> WorkspaceStore {
    let mut store = match std::fs::read_to_string(path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => WorkspaceStore::default(),
    };
    store.ensure_scratch();
    if store.active_workspace_id.is_none()
        || store.active_workspace_id.as_ref().and_then(|id| store.get(id)).is_none()
    {
        store.active_workspace_id = Some(SCRATCH_ID.to_string());
    }
    store
}

fn load_path(path: &Option<String>) -> WorkspaceStore {
    match path.as_deref() {
        Some(p) => load_from(Path::new(p)),
        None => WorkspaceStore::default(),
    }
}

/// Read-access — the guard holds a snapshot freshly read + normalised.
pub fn store(state: &CorvusState) -> MutexGuard<'static, WorkspaceStore> {
    let path = store_file_path(state);
    let mut s = STORE.lock().unwrap_or_else(|p| p.into_inner());
    *s = load_path(&path);
    s
}

/// Reload-fresh → mutate → persist, all under the lock.
pub fn mutate<T>(
    state: &CorvusState,
    f: impl FnOnce(&mut WorkspaceStore) -> Result<T, String>,
) -> Result<T, String> {
    let path = store_file_path(state);
    let mut s = STORE.lock().unwrap_or_else(|p| p.into_inner());
    *s = load_path(&path);
    let result = f(&mut s)?;
    save_to(&s, &path)?;
    Ok(result)
}

fn save_to(store: &WorkspaceStore, path: &Option<String>) -> Result<(), String> {
    let Some(path) = path else { return Ok(()); };
    let content = serde_json::to_string_pretty(store)
        .map_err(|e| format!("workspace store: serialize failed: {e}"))?;
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}
