//! Per-workspace tab snapshots (`workspace-state/<id>.json`) — owned by corvus-be.
//!
//! Ported from the shell's `crate::workspace::snapshot` (`AppError` → `String`).
//! The frontend owns tab state and pushes the full snapshot; one file per
//! workspace keeps a corrupted snapshot from blowing up the whole app. The
//! shell pushes the absolute snapshot directory via the `workspace_state_dir`
//! config section (corvus-be can't compute the profile-aware path itself).

use std::path::PathBuf;

use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};

/// Pointer to a tab opened cross-workspace — its underlying repo belongs to a
/// different workspace; we store the source workspace id for the accent dot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossWsTabRef {
    pub repo_id:      String,
    pub source_ws_id: String,
}

/// Per-tab UI metadata that outlives a restart but isn't derivable from the repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabMeta {
    pub repo_id: String,
    #[serde(default)]
    pub name_override: Option<String>,
    #[serde(default)]
    pub is_linked_worktree: bool,
}

/// Snapshot of one workspace's tab-set.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    #[serde(default)]
    pub open_tab_ids:  Vec<String>,
    #[serde(default)]
    pub active_tab_id: Option<String>,
    #[serde(default)]
    pub cross_ws_tabs: Vec<CrossWsTabRef>,
    #[serde(default)]
    pub tab_meta:      Vec<TabMeta>,
}

/// The shell-pushed absolute snapshot directory, or `None` if not yet synced.
fn snapshot_dir(state: &CorvusState) -> Option<PathBuf> {
    state
        .config("workspace_state_dir")
        .and_then(|v| v.as_str().map(PathBuf::from))
}

fn snapshot_path(state: &CorvusState, ws_id: &str) -> Option<PathBuf> {
    snapshot_dir(state).map(|d| d.join(format!("{ws_id}.json")))
}

pub fn load(state: &CorvusState, ws_id: &str) -> TabSnapshot {
    let Some(path) = snapshot_path(state, ws_id) else { return TabSnapshot::default(); };
    if !path.exists() { return TabSnapshot::default(); }
    match std::fs::read_to_string(&path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => TabSnapshot::default(),
    }
}

pub fn save(state: &CorvusState, ws_id: &str, snap: &TabSnapshot) -> Result<(), String> {
    let Some(dir) = snapshot_dir(state) else {
        // No directory pushed yet — nothing to persist to (should not happen
        // once the shell has synced).
        return Ok(());
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{ws_id}.json"));
    let content = serde_json::to_string_pretty(snap)
        .map_err(|e| format!("workspace snapshot: serialize failed: {e}"))?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete(state: &CorvusState, ws_id: &str) -> Result<(), String> {
    if let Some(path) = snapshot_path(state, ws_id) {
        if path.exists() { std::fs::remove_file(&path).map_err(|e| e.to_string())?; }
    }
    Ok(())
}
