use tauri::State;
use crate::error::AppError;
use crate::config::app_config::{self, ExplorerConfig};
use crate::AppState;

/// Persist updated file-explorer preferences. When the global-shortcut toggle
/// flips, register/unregister the OS-global `Ctrl+Shift+E` combo immediately so
/// the change takes effect without a restart.
///
/// Stays a Tauri command (keep-shell): it takes an `AppHandle` and reconciles
/// an OS-global shortcut, so it can't move to the Tauri-free platform backend.
#[tauri::command]
pub fn set_explorer_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: ExplorerConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    let old_explorer = cfg.explorer.clone();
    cfg.explorer = config;
    let new_explorer = cfg.explorer.clone();
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    // Apply the global-shortcut change immediately; a registration conflict
    // (invalid / already-claimed combo) surfaces to the UI so it can revert.
    #[cfg(desktop)]
    crate::explorer_window::reconcile_global_shortcut(&app, &old_explorer, &new_explorer)
        .map_err(AppError::Other)?;
    Ok(())
}
