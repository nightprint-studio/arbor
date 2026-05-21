/// Shared helpers for reading and writing plugin settings JSON files.
///
/// Two storage scopes exist:
///   - **Plugin-level** (`settings.json`): generic key/value edited through the
///     Arbor UI (Settings → Plugins). Path: `~/.config/arbor/plugin_data/<name>/settings.json`.
///   - **Global / Project** (`global.json` / `.arbor/plugins/<name>/project.json`):
///     written by plugins via the `arbor.settings.*` Lua API.
///
/// All I/O helpers silently return an empty map on read failure and log a warning
/// on write failure instead of panicking or silently discarding errors.

use std::path::{Path, PathBuf};

pub type SettingsMap = serde_json::Map<String, serde_json::Value>;

// ---------------------------------------------------------------------------
// Path builders
// ---------------------------------------------------------------------------

/// Path to the global settings file for a plugin
/// (`~/.config/arbor/plugin_data/<name>/global.json`).
///
/// All persistent plugin data lives here now. The legacy `settings.json`
/// (owned by the old `[[setting]]` manifest schema) was removed when
/// settings were converted to the contribution model; if such a file
/// still exists from an old install it is simply ignored.
///
/// Dev builds use `plugin_data-dev/` so a debug session doesn't clobber a
/// stable Arbor's per-plugin settings.
pub fn global_settings_path(plugin_name: &str) -> PathBuf {
    let data_subdir = if cfg!(debug_assertions) { "plugin_data-dev" } else { "plugin_data" };
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join(data_subdir)
        .join(plugin_name)
        .join("global.json")
}

/// Path to the per-repo project settings file for a plugin
/// (`<repo>/.arbor/plugins/<name>/project.json`).
pub fn project_settings_path(plugin_name: &str, repo_path: &str) -> PathBuf {
    PathBuf::from(repo_path)
        .join(".arbor")
        .join("plugins")
        .join(plugin_name)
        .join("project.json")
}

// ---------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------

/// Read a JSON settings file into a `SettingsMap`. Returns an empty map when
/// the file does not exist or cannot be parsed (logs a warning in the latter case).
pub fn load_settings_file(path: &Path) -> SettingsMap {
    if !path.exists() {
        return SettingsMap::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(serde_json::Value::Object(map)) => map,
            Ok(_) => {
                tracing::warn!("settings file {} is not a JSON object, ignoring", path.display());
                SettingsMap::new()
            }
            Err(e) => {
                tracing::warn!("failed to parse settings file {}: {e}", path.display());
                SettingsMap::new()
            }
        },
        Err(e) => {
            tracing::warn!("failed to read settings file {}: {e}", path.display());
            SettingsMap::new()
        }
    }
}

/// Write a `SettingsMap` to a JSON file, creating parent directories as needed.
/// Logs a warning on any I/O or serialization error instead of silently discarding.
pub fn save_settings_file(path: &Path, map: &SettingsMap) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("failed to create settings dir {}: {e}", parent.display());
            return;
        }
    }
    match serde_json::to_string_pretty(&serde_json::Value::Object(map.clone())) {
        Ok(content) => {
            if let Err(e) = std::fs::write(path, &content) {
                tracing::warn!("failed to write settings file {}: {e}", path.display());
            }
        }
        Err(e) => tracing::warn!("failed to serialize settings for {}: {e}", path.display()),
    }
}
