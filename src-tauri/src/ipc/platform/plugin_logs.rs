//! `plugin_logs` domain — read / append / clear the in-memory plugin log ring buffer.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline, now
//! self-registered under `program = "platform"`. Behavior (locks held, buffer
//! mutation) is byte-identical; filtering by plugin / level stays on the
//! frontend so the backend can remain write-once. No hooks fire in this domain.
//!
//! The one write is [`record_plugin_log`]: half a plugin's failures happen in
//! the webview — an action the plugin fired that came back rejected, a payload
//! it sent that would not render — and the panel that exists to show plugin
//! failures cannot be reached from there without a door.

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

/// Append one entry, from the frontend.
///
/// The counterpart of `AppCtx::record_plugin_log`, for the half of a plugin's
/// surface that lives in the webview. Frontend callers should go through
/// `reportPluginError` / `reportPluginWarning` rather than here directly, so
/// the console keeps getting the line too.
///
/// `level` is taken as written and normalised on read; an unrecognised value
/// simply never matches a level filter, which is a filtering problem and not a
/// reason to drop somebody's error.
#[platform::handler(program = "platform")]
fn record_plugin_log(
    state:   &AppState,
    level:   String,
    plugin:  String,
    message: String,
) -> Result<(), AppError> {
    // Not `plugin_logs::record`: that one emits through an `AppHandle`, and a platform
    // handler holds `&AppState`. The sink is absent only during early boot, before the
    // backend is wired — and nothing in the webview is firing plugin actions yet.
    let Some(sink) = state.event_sink() else { return Ok(()) };
    crate::plugin_logs::record_via(&state.plugin_logs, &sink, &level, &plugin, message);
    Ok(())
}
