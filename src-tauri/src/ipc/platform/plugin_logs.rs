//! `plugin_logs` domain — read / clear the in-memory plugin log ring buffer.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline, now
//! self-registered under `program = "platform"`. Behavior (locks held, buffer
//! mutation) is byte-identical; filtering by plugin / level stays on the
//! frontend so the backend can remain write-once. No hooks fire in this domain.

use crate::error::AppError;
use crate::ipc::platform;
use crate::plugin_logs::PluginLogEntry;
use crate::AppState;

/// Snapshot of every entry currently in the ring buffer (oldest → newest).
/// Filtering by plugin / level happens on the frontend so the backend can
/// stay write-once.
#[platform::handler(program = "platform")]
fn list_plugin_logs(state: &AppState) -> Result<Vec<PluginLogEntry>, AppError> {
    let buf = state.lock_plugin_logs()?;
    Ok(buf.snapshot())
}

/// Drop every entry — the next stream event re-seeds an empty list on the
/// frontend.  Useful when the panel becomes overwhelming or the user wants
/// a clean slate before reproducing a bug.
#[platform::handler(program = "platform")]
fn clear_plugin_logs(state: &AppState) -> Result<(), AppError> {
    let mut buf = state.lock_plugin_logs()?;
    buf.clear();
    Ok(())
}

/// Drop every entry tagged with the given pipeline name. Used by the
/// "Clear pipeline logs" affordance in the panel — lets the user nuke a
/// noisy run's mirrored output without wiping plain `arbor.log.*`
/// entries from the rest of the session.
#[platform::handler(program = "platform")]
fn clear_plugin_logs_by_pipeline(state: &AppState, name: String) -> Result<(), AppError> {
    let mut buf = state.lock_plugin_logs()?;
    buf.clear_by_pipeline(&name);
    Ok(())
}
