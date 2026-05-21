use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Pointer to a tab that was opened cross-workspace (i.e. the underlying
/// repo belongs to a different workspace).  We store the source workspace id
/// so the UI can render the accent dot in the right colour and, later, let
/// the user "switch to source workspace".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossWsTabRef {
    pub repo_id:       String,
    pub source_ws_id:  String,
}

/// Per-tab UI metadata that has to outlive a process restart but is not
/// derivable from the underlying repo (it only exists when the user has done
/// something to the tab, e.g. swapped its worktree path).  Keyed by repo_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabMeta {
    pub repo_id: String,
    /// Tab name shown in the bar.  None means "use the default repo name".
    /// Set by the worktree switcher so swapping `/repo` → `/repo-feat`
    /// keeps the original "repo" label.
    #[serde(default)]
    pub name_override: Option<String>,
    /// True when the tab points at a non-main worktree of its underlying
    /// repo — drives the worktree icon next to the tab name.
    #[serde(default)]
    pub is_linked_worktree: bool,
}

/// Snapshot of one workspace's tab-set.  One file per workspace keeps a
/// corrupted snapshot from blowing up the whole app.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    /// Ordered list of repo ids currently open in this workspace, including
    /// cross-workspace tabs (their ids appear here AND in `cross_ws_tabs`).
    /// Ordering is the user's tab order.
    #[serde(default)]
    pub open_tab_ids:  Vec<String>,
    #[serde(default)]
    pub active_tab_id: Option<String>,
    #[serde(default)]
    pub cross_ws_tabs: Vec<CrossWsTabRef>,
    /// Optional per-tab UI metadata.  Only tabs with non-default state
    /// appear here, so older snapshots without this field still load.
    #[serde(default)]
    pub tab_meta:     Vec<TabMeta>,
}

fn snapshot_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("workspace-state")
}

fn snapshot_path(ws_id: &str) -> PathBuf {
    snapshot_dir().join(format!("{ws_id}.json"))
}

pub fn load(ws_id: &str) -> TabSnapshot {
    let path = snapshot_path(ws_id);
    if !path.exists() { return TabSnapshot::default(); }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("workspace snapshot [{ws_id}]: read failed: {e}");
            return TabSnapshot::default();
        }
    };
    serde_json::from_str(&content).unwrap_or_default()
}

pub fn save(ws_id: &str, snap: &TabSnapshot) -> Result<()> {
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir)?;
    let path = snapshot_path(ws_id);
    let content = serde_json::to_string_pretty(snap)
        .map_err(|e| AppError::Other(format!("workspace snapshot: serialize failed: {e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn delete(ws_id: &str) -> Result<()> {
    let path = snapshot_path(ws_id);
    if path.exists() { std::fs::remove_file(&path)?; }
    Ok(())
}
