use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};

/// Fixed id for the implicit "Scratch" workspace that every installation
/// always has.  Non-deletable, non-renameable; shown at the bottom of the
/// workspace dropdown.  Its colour index is editable.
pub const SCRATCH_ID: &str = "scratch";

/// Reserved for a future per-workspace git identity override.  Unused in v1
/// but baked into the schema so adding it later does not require migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitIdentity {
    pub name:  String,
    pub email: String,
}

/// Optional visual parent for one or more workspaces.  Groups are a pure
/// UI organisation aid (think "Clients" / "Internal tools" / …) — they
/// don't alter the behaviour of the workspaces they contain.  Collapsing a
/// group in the dropdown hides all its children at once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGroup {
    pub id:        String,
    pub name:      String,
    #[serde(default)]
    pub order:     u32,
    /// Persisted collapse state.  `true` = children hidden in dropdown.
    #[serde(default)]
    pub collapsed: bool,
    /// Optional colour index for the group label itself.  Kept separate
    /// from the child workspaces' colours so visually nothing forces them
    /// to share a hue.
    #[serde(default)]
    pub color_idx: u8,
}

/// A workspace — ordered group of repo ids plus some cosmetic data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDef {
    pub id:        String,
    pub name:      String,
    /// Index into the CSS palette (`--ws-color-0`..`--ws-color-11`).  Stored
    /// as an index (not RGB) so theme switches automatically restyle every
    /// existing workspace without touching persisted state.
    #[serde(default)]
    pub color_idx: u8,
    pub repo_ids:  Vec<String>,
    /// Ordering inside the dropdown / management modal.  Kept explicit so
    /// drag-reorder survives process restarts.
    #[serde(default)]
    pub order:     u32,

    /// Optional parent group (purely visual).  `None` = top level.  Scratch
    /// always stays ungrouped.
    #[serde(default)]
    pub group_id:          Option<String>,

    // ── Reserved extensibility fields (unused v1, persisted so we don't
    // need a migration to start using them later). ──────────────────────────
    #[serde(default)]
    pub metadata:          serde_json::Value,
    #[serde(default)]
    pub settings_override: Option<serde_json::Value>,
    #[serde(default)]
    pub git_identity:      Option<GitIdentity>,
}

impl WorkspaceDef {
    pub fn is_scratch(&self) -> bool { self.id == SCRATCH_ID }
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

    pub fn create(
        &mut self,
        name: String,
        color_idx: u8,
        repo_ids: Vec<String>,
        group_id: Option<String>,
    ) -> WorkspaceDef {
        // Non-scratch workspaces use order values below u32::MAX so scratch
        // stays pinned at the bottom.  We append with (current max non-scratch
        // order + 1) so creation preserves insertion order.
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

    pub fn remove(&mut self, id: &str) -> Result<()> {
        if id == SCRATCH_ID {
            return Err(AppError::Other("cannot delete the Scratch workspace".into()));
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

    pub fn add_repo(&mut self, workspace_id: &str, repo_id: &str) -> Result<()> {
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        if !ws.repo_ids.iter().any(|id| id == repo_id) {
            ws.repo_ids.push(repo_id.to_string());
        }
        Ok(())
    }

    pub fn remove_repo(&mut self, workspace_id: &str, repo_id: &str) -> Result<()> {
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.repo_ids.retain(|id| id != repo_id);
        Ok(())
    }

    /// Fully purge a repo id from every workspace.  Used when the user
    /// deletes a repo from the registry — we don't want dangling ids.
    pub fn purge_repo_everywhere(&mut self, repo_id: &str) {
        for ws in &mut self.workspaces {
            ws.repo_ids.retain(|id| id != repo_id);
        }
    }

    /// True when at least one workspace (including Scratch) lists `repo_id`.
    /// Used to detect "removed from last workspace" so we can fire the
    /// `on_repo_deregistered` hook for orphaned registry entries.
    pub fn repo_is_in_any_workspace(&self, repo_id: &str) -> bool {
        self.workspaces.iter().any(|ws| ws.repo_ids.iter().any(|id| id == repo_id))
    }

    /// Return workspaces sorted by their `order` field (Scratch always last
    /// because its order is u32::MAX).
    pub fn ordered(&self) -> Vec<WorkspaceDef> {
        let mut v = self.workspaces.clone();
        v.sort_by_key(|w| (w.order, w.name.to_lowercase()));
        v
    }

    // ── Group management ──────────────────────────────────────────────────
    //
    // Groups are a purely visual organisation aid — they never affect the
    // set of open tabs or which workspace is active.  The backend keeps
    // their list and the persisted collapsed state; everything else is
    // rendered by the UI.

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

    pub fn remove_group(&mut self, id: &str) -> Result<()> {
        // Orphan children — they reappear at top level, we don't cascade-delete.
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

    pub fn set_workspace_group(&mut self, workspace_id: &str, group_id: Option<String>) -> Result<()> {
        if workspace_id == SCRATCH_ID {
            return Err(AppError::Other("Scratch cannot be placed inside a group".into()));
        }
        // If the target group doesn't exist, treat as ungrouped rather than fail.
        let resolved = group_id.filter(|id| !id.is_empty())
            .and_then(|id| if self.get_group(&id).is_some() { Some(id) } else { None });
        let ws = self.get_mut(workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.group_id = resolved;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("workspaces.json")
}

pub fn load() -> WorkspaceStore {
    let path = store_path();
    if !path.exists() { return WorkspaceStore::default(); }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("workspace store: read failed, starting empty: {e}");
            return WorkspaceStore::default();
        }
    };
    let mut store: WorkspaceStore = serde_json::from_str(&content).unwrap_or_default();
    store.ensure_scratch();
    if store.active_workspace_id.is_none()
        || store.active_workspace_id.as_ref()
              .and_then(|id| store.get(id)).is_none()
    {
        store.active_workspace_id = Some(SCRATCH_ID.to_string());
    }
    store
}

pub fn save(store: &WorkspaceStore) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let content = serde_json::to_string_pretty(store)
        .map_err(|e| AppError::Other(format!("workspace store: serialize failed: {e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}
