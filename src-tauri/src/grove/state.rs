//! Persisted grove **window** state — recents, last project, panel layout.
//!
//! A dedicated file (`<data>/arbor/grove/state.json`), deliberately **not** the
//! typed `[grove]` config (those are engine settings), **not** the per-project
//! `grove.toml` (that's the project model), and **not** `localStorage` (hard rule
//! #11). This is global, app-level window state. Missing / unparseable → defaults,
//! so a first launch or a corrupt file just starts clean.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Persisted panel layout of the grove window.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GroveLayoutState {
    /// `files` | `outline` | `soundbank` | null.
    pub left_panel: Option<String>,
    /// `console` | `problems` | `mixer` | null.
    pub bottom_panel: Option<String>,
    /// `inspector` | `docs` | null.
    pub right_panel: Option<String>,
    /// Arrangement (viz) pane hidden.
    pub collapse_viz: bool,
    /// Editor pane hidden.
    pub collapse_editor: bool,
}

/// The dedicated grove window state file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GroveWorkspaceState {
    /// Recently-opened project folders, most-recent first.
    pub recent_projects: Vec<String>,
    /// Project folder to reopen on launch, or `None`.
    pub last_project: Option<String>,
    /// The window's panel arrangement.
    pub layout: GroveLayoutState,
}

/// `<data>/arbor/grove/state.json`.
fn state_path() -> PathBuf {
    arbor_core::prelude::arbor_data_dir()
        .join("grove")
        .join("state.json")
}

/// Read the persisted grove window state. A missing or unreadable/unparseable
/// file yields defaults (clean start), never an error.
#[tauri::command]
pub fn get_grove_state() -> Result<GroveWorkspaceState, AppError> {
    let path = state_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(GroveWorkspaceState::default());
    };
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

/// Persist the grove window state (pretty JSON), creating the dir if needed.
#[tauri::command]
pub fn set_grove_state(state: GroveWorkspaceState) -> Result<(), AppError> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    }
    let text = serde_json::to_string_pretty(&state).map_err(|e| AppError::Other(e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| AppError::Other(e.to_string()))
}
