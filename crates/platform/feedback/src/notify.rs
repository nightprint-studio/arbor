//! In-app notification contract: the `plugin:notification` payload + emit
//! helper. The Lua binding (`arbor.notify`) validates user input and then
//! builds a [`NotificationPayload`] and hands it to [`emit_notification`].
//!
//! The frontend listener interleaves these into the bottom-right feed and the
//! bell archive. `level` is one of `info | success | warning | error` —
//! validated at the binding boundary, kept as a `String` here.

use arbor_core::prelude::AppCtx;
use serde::Serialize;

/// Frontend event name the notification host listens on.
pub const EVENT_NOTIFICATION: &str = "plugin:notification";

#[derive(Debug, Clone, Serialize)]
pub struct NotificationPayload {
    /// Emitting plugin's name (or a built-in source label).
    pub plugin: String,
    pub title: String,
    pub message: String,
    /// `info | success | warning | error` (validated upstream).
    pub level: String,
    /// Show a transient toast in the bottom-right feed.
    pub toast: bool,
    /// Add to the persistent bell archive.
    pub persist: bool,
    /// Optional click-action descriptor (shape validated by the frontend).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<serde_json::Value>,
    /// Optional window-routing target. `None` → main window (which also
    /// receives untagged items); a value routes to the matching feedback host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Emit a notification to the frontend over the host's event bus.
pub fn emit_notification(ctx: &dyn AppCtx, payload: &NotificationPayload) {
    let value = serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
    ctx.emit(EVENT_NOTIFICATION, value);
}
