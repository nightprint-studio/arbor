//! A backend asking for the user when they may not be looking: a system notification and a request for
//! attention (the dock icon bouncing, the taskbar button flashing).
//!
//! Backends announce it on [`arbor_ipc::prelude::ATTENTION_TOPIC`] — today Imber stopping on a question and,
//! when asked for, a run that ended. Whether to announce is the backend's setting; whether to *show* is decided
//! here, because only the shell knows which window has the focus: someone looking at the product's window, or
//! at the workspace window that hosts it, sees the card already, and a notification on top of it is noise.

use serde_json::Value;
use tauri::{AppHandle, Manager, UserAttentionType, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

/// Show what `program`'s backend asked for, unless one of its windows is in front.
pub fn notify(app: &AppHandle, program: &str, payload: &Value) {
    let windows = windows_of(app, program);
    if windows.iter().any(|w| w.is_focused().unwrap_or(false)) {
        return;
    }
    let field = |k: &str| payload.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let title = field("title");
    if title.is_empty() {
        return;
    }
    let body = field("body");
    let mut note = app.notification().builder().title(&title);
    if !body.is_empty() {
        note = note.body(&body);
    }
    if let Err(e) = note.show() {
        eprintln!("[attention] the system notification could not be shown: {e}");
    }
    // Critical keeps asking (the dock icon bounces) until the user comes to the app — a question is waiting.
    if let Some(window) = windows.first() {
        let _ = window.request_user_attention(Some(UserAttentionType::Critical));
    }
}

/// The windows that show `program`: its own (`bennu`, `bennu-2`, …) first, then the workspace window that can host
/// any product.
fn windows_of(app: &AppHandle, program: &str) -> Vec<WebviewWindow> {
    let prefix = format!("{program}-");
    let mut own = Vec::new();
    let mut hosts = Vec::new();
    for (label, window) in app.webview_windows() {
        if label == program || label.starts_with(&prefix) {
            own.push(window);
        } else if label == crate::window::workspace::WORKSPACE_WINDOW_LABEL {
            hosts.push(window);
        }
    }
    own.extend(hosts);
    own
}
