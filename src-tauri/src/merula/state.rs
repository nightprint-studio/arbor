// TODO(prune): merula moved to merula-be — these commands are no longer registered.
//! Persisted merula **window** state — recents, last project, panel layout,
//! sound-bank favourites/recents, and open-tab snapshots.
//!
//! Global window state lives in `<merula-config>/state.json`; the **scratch** tabs
//! are global too (`<merula-config>/scratch.json`); per-project **open editor tabs**
//! live next to the project in `<project>/.merula/tabs.json`, so a project carries
//! its own session. Deliberately **not** the typed `[merula]` config (engine
//! settings), **not** the per-project `merula.toml` (the project model), and
//! **not** `localStorage` (hard rule #11). Missing / unparseable → defaults, so a
//! first launch or a corrupt file just starts clean.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::error::AppError;

/// Persisted panel layout of the merula window.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MerulaLayoutState {
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

/// One named project workspace — an Arbor-style group of `.merula` projects with a
/// colour, switchable from the title bar.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MerulaProjectWorkspace {
    /// Stable id (generated on the FE).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Index into the FE workspace colour palette.
    pub color_idx: u32,
    /// Member project folders (absolute paths).
    pub project_paths: Vec<String>,
}

/// The dedicated merula window state file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MerulaWorkspaceState {
    /// Recently-opened project folders, most-recent first.
    pub recent_projects: Vec<String>,
    /// Project folder to reopen on launch, or `None`.
    pub last_project: Option<String>,
    /// The window's panel arrangement.
    pub layout: MerulaLayoutState,
    /// Sound-bank favourites (instrument names), no particular order.
    pub favorite_sounds: Vec<String>,
    /// Recently-used instrument names, most-recent first.
    pub recent_sounds: Vec<String>,
    /// Named project workspaces (groups of `.merula` projects).
    pub workspaces: Vec<MerulaProjectWorkspace>,
    /// The active workspace id, or `None` (no workspace selected).
    pub active_workspace: Option<String>,
}

// ── Generic JSON file helpers ────────────────────────────────────────────────

/// Read + parse a JSON file, falling back to the type's default when the file is
/// missing or unparseable (a clean start, never an error).
fn read_json<T: Default + DeserializeOwned>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Write a value as pretty JSON, creating the parent directory if needed.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| AppError::Other(e.to_string()))?;
    std::fs::write(path, text).map_err(|e| AppError::Other(e.to_string()))
}

// ── Global window state (`<merula-config>/state.json`) ─────────────────────────

/// `<merula-config>/state.json`. Per-profile, NOT under the global data dir (which
/// now holds the shared heavy assets), so each profile gets its own window state.
fn state_path() -> PathBuf {
    arbor_core::prelude::merula_config_dir().join("state.json")
}

/// Read the persisted merula window state (defaults on a missing/corrupt file).
#[tauri::command]
pub fn get_merula_state() -> Result<MerulaWorkspaceState, AppError> {
    Ok(read_json(&state_path()))
}

/// Persist the merula window state (pretty JSON), creating the dir if needed.
#[tauri::command]
pub fn set_merula_state(state: MerulaWorkspaceState) -> Result<(), AppError> {
    write_json(&state_path(), &state)
}

// ── Per-project open tabs (`<project>/.merula/tabs.json`) ──────────────────────

/// The open editor tabs of a project, restored when it's reopened.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MerulaProjectTabs {
    /// Absolute paths of the open `.merula` tabs, in tab order.
    pub open_file_paths: Vec<String>,
    /// The active tab's path, or `None`.
    pub active_file_path: Option<String>,
}

fn project_tabs_path(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".merula").join("tabs.json")
}

/// Read a project's open-tab snapshot (defaults to none on first open).
#[tauri::command]
pub fn get_merula_project_tabs(project_path: String) -> Result<MerulaProjectTabs, AppError> {
    Ok(read_json(&project_tabs_path(&project_path)))
}

/// Persist a project's open-tab snapshot under its own `.merula/` folder.
#[tauri::command]
pub fn set_merula_project_tabs(
    project_path: String,
    tabs: MerulaProjectTabs,
) -> Result<(), AppError> {
    write_json(&project_tabs_path(&project_path), &tabs)
}

// ── Per-project mix state (`<project>/.merula/mix.json`) ───────────────────────
//
// Master gain + shared reverb-return decay have NO `.merula` source representation
// (they're mixer-only, session-level), so without this they reset to defaults on
// every reopen. Persisted next to the project so a song carries its master mix.

/// A project's persisted master-bus mix (no source representation).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MerulaProjectMix {
    /// Master output gain (0..1, linear). Default unity.
    pub master_gain: f32,
    /// Shared reverb-return decay in seconds. Default 0.5.
    pub reverb_decay: f32,
}

impl Default for MerulaProjectMix {
    fn default() -> Self {
        MerulaProjectMix { master_gain: 1.0, reverb_decay: 0.5 }
    }
}

fn project_mix_path(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".merula").join("mix.json")
}

/// Read a project's master mix (defaults to unity / 0.5s on first open).
#[tauri::command]
pub fn get_merula_project_mix(project_path: String) -> Result<MerulaProjectMix, AppError> {
    Ok(read_json(&project_mix_path(&project_path)))
}

/// Persist a project's master mix under its own `.merula/` folder.
#[tauri::command]
pub fn set_merula_project_mix(
    project_path: String,
    mix: MerulaProjectMix,
) -> Result<(), AppError> {
    write_json(&project_mix_path(&project_path), &mix)
}

// ── Global sound aliases (`<merula-config>/aliases.json`) ──────────────────────
//
// User-defined `alias → target` name map (e.g. `kick = "RolandTR808_bd"`),
// resolved by the audio registry so `s("kick")` plays the target. Global (NOT
// per-project / per-file) but per-profile, so it's a dedicated config file the
// engine reads when building a session registry.

fn aliases_path() -> PathBuf {
    arbor_core::prelude::merula_config_dir().join("aliases.json")
}

/// Read the global sound-alias map (defaults to empty on first run / corrupt file).
/// Also used by the registry builder, not just the command.
pub fn load_aliases() -> HashMap<String, String> {
    read_json(&aliases_path())
}

/// Read the global sound-alias map (`alias → target`).
#[tauri::command]
pub fn get_merula_aliases() -> Result<HashMap<String, String>, AppError> {
    Ok(load_aliases())
}

/// Persist the global sound-alias map. Takes effect on the next eval / session
/// rebuild (the registry builder re-reads this file).
#[tauri::command]
pub fn set_merula_aliases(aliases: HashMap<String, String>) -> Result<(), AppError> {
    write_json(&aliases_path(), &aliases)
}

// ── Scratch tabs (global, `<merula-config>/scratch.json`) ──────────────────────

/// One persisted scratch tab (the transient eval result is **not** saved).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MerulaScratchTab {
    pub id: String,
    pub name: String,
    pub source: String,
}

/// The scratch workspace: the tabs + which one was active.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MerulaScratchTabs {
    pub tabs: Vec<MerulaScratchTab>,
    pub active_id: Option<String>,
}

fn scratch_tabs_path() -> PathBuf {
    arbor_core::prelude::merula_config_dir().join("scratch.json")
}

/// Read the persisted scratch tabs (defaults to none).
#[tauri::command]
pub fn get_merula_scratch_tabs() -> Result<MerulaScratchTabs, AppError> {
    Ok(read_json(&scratch_tabs_path()))
}

/// Persist the scratch tabs (global, in the per-profile merula config dir).
#[tauri::command]
pub fn set_merula_scratch_tabs(tabs: MerulaScratchTabs) -> Result<(), AppError> {
    write_json(&scratch_tabs_path(), &tabs)
}
