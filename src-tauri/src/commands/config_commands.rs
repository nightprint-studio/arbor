use tauri::State;
use crate::error::AppError;
use crate::config::app_config::{self, ExplorerConfig, LauncherConfig, TytoConfig};
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
    crate::window::explorer::reconcile_global_shortcut(&app, &old_explorer, &new_explorer)
        .map_err(AppError::Other)?;
    Ok(())
}

/// Persist updated Tyto (screen recorder) preferences. When the global-shortcut
/// toggle or accelerator changes, register/unregister the OS-global combo
/// immediately so the change takes effect without a restart.
///
/// Stays a Tauri command (keep-shell): it takes an `AppHandle` and reconciles an
/// OS-global shortcut, so it can't move to the Tauri-free platform backend.
/// Mirrors [`set_explorer_config`].
#[tauri::command]
pub fn set_tyto_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: TytoConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    let old_tyto = cfg.tyto.clone();
    cfg.tyto = config;
    let new_tyto = cfg.tyto.clone();
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    #[cfg(desktop)]
    crate::window::tyto::reconcile_global_shortcut(&app, &old_tyto, &new_tyto)
        .map_err(AppError::Other)?;
    Ok(())
}

/// Read launcher (Canopy) preferences (the per-product map). Stays a keep-shell
/// command: the launcher is a shell concern and the close-to-tray flags are read
/// by the native window-event handler.
#[tauri::command]
pub fn get_launcher_config(state: State<'_, AppState>) -> Result<LauncherConfig, AppError> {
    Ok(state.lock_config()?.launcher.clone())
}

/// Choose where workspace products open: one window each, or tabs in the
/// shared container. Takes effect on the next product launch — windows that are
/// already open stay where they are.
#[tauri::command]
pub fn set_launcher_window_mode(
    state: State<'_, AppState>,
    mode: app_config::WindowMode,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.launcher.window_mode = mode;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

/// Set a single product's tray-close preference (per-product, not global).
#[tauri::command]
pub fn set_launcher_close_to_tray(
    state: State<'_, AppState>,
    id: String,
    close_to_tray: bool,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.launcher.products.entry(id).or_default().close_to_tray = close_to_tray;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}
