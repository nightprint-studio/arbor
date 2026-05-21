use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTab {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub tabs: Vec<PersistedTab>,
    pub active_path: Option<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self { tabs: vec![], active_path: None }
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn session_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("session.json")
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Persist the current list of open tabs and the active repo path.
/// Called by the frontend on every tab mutation (add, remove, activate).
#[tauri::command]
pub fn save_session(
    tabs: Vec<PersistedTab>,
    active_path: Option<String>,
) -> Result<(), AppError> {
    let session = SessionState { tabs, active_path };
    let path = session_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("session: cannot create config dir: {e}")))?;
    }

    let json = serde_json::to_string_pretty(&session)
        .map_err(|e| AppError::Other(format!("session: serialization failed: {e}")))?;

    std::fs::write(&path, json)
        .map_err(|e| AppError::Other(format!("session: write failed: {e}")))?;

    Ok(())
}

/// Load the persisted session from disk.
/// Returns an empty session (no tabs, no active path) if the file doesn't exist.
#[tauri::command]
pub fn load_session() -> Result<SessionState, AppError> {
    let path = session_path();

    if !path.exists() {
        return Ok(SessionState::default());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Other(format!("session: read failed: {e}")))?;

    let session: SessionState = serde_json::from_str(&content)
        .unwrap_or_default(); // corrupt file → start fresh, no panic

    Ok(session)
}
