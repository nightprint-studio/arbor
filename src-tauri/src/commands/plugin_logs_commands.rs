use tauri::State;
use crate::AppState;
use crate::error::AppError;
use crate::plugin_logs::PluginLogEntry;

/// Snapshot of every entry currently in the ring buffer (oldest → newest).
/// Filtering by plugin / level happens on the frontend so the backend can
/// stay write-once.
#[tauri::command]
pub fn list_plugin_logs(state: State<'_, AppState>) -> Result<Vec<PluginLogEntry>, AppError> {
    let buf = state.lock_plugin_logs()?;
    Ok(buf.snapshot())
}

/// Drop every entry — the next stream event re-seeds an empty list on the
/// frontend.  Useful when the panel becomes overwhelming or the user wants
/// a clean slate before reproducing a bug.
#[tauri::command]
pub fn clear_plugin_logs(state: State<'_, AppState>) -> Result<(), AppError> {
    let mut buf = state.lock_plugin_logs()?;
    buf.clear();
    Ok(())
}

/// Drop every entry tagged with the given pipeline name. Used by the
/// "Clear pipeline logs" affordance in the panel — lets the user nuke a
/// noisy run's mirrored output without wiping plain `arbor.log.*`
/// entries from the rest of the session.
#[tauri::command]
pub fn clear_plugin_logs_by_pipeline(
    state: State<'_, AppState>,
    name:  String,
) -> Result<(), AppError> {
    let mut buf = state.lock_plugin_logs()?;
    buf.clear_by_pipeline(&name);
    Ok(())
}
