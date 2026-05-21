use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::AppError;
use crate::config::app_config;
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
/// SettingsPanel) and marketplace-installed theme JSONs. Mirrors the
/// `-dev` suffix the other marketplace state files use so a debug-build
/// Arbor never poisons the release install (or vice versa).
pub fn themes_dir() -> PathBuf {
    let filename = if cfg!(debug_assertions) {
        "themes-dev"
    } else {
        "themes"
    };
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join(filename)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// List all user-created custom themes stored in ~/.config/arbor/themes/
#[tauri::command]
pub fn list_custom_themes() -> Result<Vec<ThemeData>, AppError> {
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
#[tauri::command]
pub fn get_active_theme_id(state: State<'_, AppState>) -> Result<String, AppError> {
    let config = state.lock_config()?;
    Ok(config.theme.active.clone())
}

/// Persist the active theme ID to the app config.
#[tauri::command]
pub fn set_active_theme_id(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.theme.active = id;
    let clone = cfg.clone();
    drop(cfg);
    app_config::save(&clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Save (create or update) a custom theme JSON file on disk.
#[tauri::command]
pub fn save_custom_theme(theme: ThemeData) -> Result<(), AppError> {
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
#[tauri::command]
pub fn delete_custom_theme(id: String) -> Result<(), AppError> {
    let path = themes_dir().join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}
