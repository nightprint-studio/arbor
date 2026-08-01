//! `theme` domain — custom-theme CRUD and the active-theme-id config field,
//! routed through the in-process `platform` backend.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[platform::handler(program = "platform")]` self-registers it under its own
//! function name. The on-disk theme directory + serde shapes are unchanged, so
//! the FE decodes identically whether the call routes here or through a legacy
//! command.
//!
//! `list_custom_themes` / `save_custom_theme` / `delete_custom_theme` never
//! touched `AppState`, but the handler macro requires a context first arg, so
//! they take `_state: &AppState` and ignore it.
//!
//! `notify_theme_changed` is **not** here: it broadcasts the `arbor:theme_changed`
//! hook to every loaded plugin and lives in `branding_commands.rs`, so it stays
//! inline for the later emit/hook seam pass. No hooks fire in this domain.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use arbor_plugin_marketplace::prelude as mk;

use crate::config::app_config;
use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

// ---------------------------------------------------------------------------
// Theme data type — mirrors the JSON files in src/lib/themes/
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeData {
    pub id:          String,
    pub name:        String,
    pub description: Option<String>,
    pub built_in:    bool,
    pub vars:        std::collections::HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Directory holding both user-created custom themes (saved via the
/// SettingsPanel) and marketplace-installed theme JSONs. Single source of
/// truth lives in `arbor-plugin-marketplace::paths::themes_dir` — the
/// marketplace installer writes to the same folder this dir-lister reads
/// back, so they must agree on the `-dev` suffix.
pub fn themes_dir() -> PathBuf { mk::themes_dir() }

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// List all user-created custom themes stored in ~/.config/arbor/themes/
#[platform::handler(program = "platform")]
fn list_custom_themes(_state: &AppState) -> Result<Vec<ThemeData>, AppError> {
    let dir = themes_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut themes = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| AppError::Other(e.to_string()))?
    {
        let entry = entry.map_err(|e| AppError::Other(e.to_string()))?;
        let path  = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(e.to_string()))?;
        match serde_json::from_str::<ThemeData>(&content) {
            Ok(t) => themes.push(t),
            Err(e) => tracing::warn!("skipping invalid theme {:?}: {e}", path),
        }
    }
    // Stable order by name
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(themes)
}

/// Return the currently active theme ID from the app config.
#[platform::handler(program = "platform")]
fn get_active_theme_id(state: &AppState) -> Result<String, AppError> {
    let config = state.lock_config()?;
    Ok(config.theme.active.clone())
}

/// Persist the active theme ID to the app config.
#[platform::handler(program = "platform")]
fn set_active_theme_id(state: &AppState, id: String) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.theme.active = id;
    let clone = cfg.clone();
    drop(cfg);
    app_config::save(&clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Save (create or update) a custom theme JSON file on disk.
#[platform::handler(program = "platform")]
fn save_custom_theme(_state: &AppState, theme: ThemeData) -> Result<(), AppError> {
    if theme.built_in {
        return Err(AppError::Other("cannot overwrite a built-in theme".into()));
    }
    // Basic ID validation — only alphanumeric, dashes and underscores.
    if !theme.id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::Other("theme id must be alphanumeric (dashes/underscores ok)".into()));
    }
    let dir = themes_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Other(e.to_string()))?;
    let path    = dir.join(format!("{}.json", theme.id));
    let content = serde_json::to_string_pretty(&theme)
        .map_err(|e| AppError::Other(e.to_string()))?;
    std::fs::write(path, content).map_err(|e| AppError::Other(e.to_string()))
}

/// Delete a custom theme JSON file from disk.
#[platform::handler(program = "platform")]
fn delete_custom_theme(_state: &AppState, id: String) -> Result<(), AppError> {
    let path = themes_dir().join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}
