use tauri::{AppHandle, Emitter};

/// Emit a Tauri event from a plugin to the frontend.
///
/// `event_name` is prefixed with `plugin:` to namespace plugin events.
#[allow(dead_code)]
pub fn emit(app: &AppHandle, plugin_name: &str, event_name: &str, payload: &str) {
    let full_event = format!("plugin:{plugin_name}:{event_name}");
    if let Err(e) = app.emit(&full_event, payload) {
        tracing::warn!("event_bus emit error: {e}");
    }
}
