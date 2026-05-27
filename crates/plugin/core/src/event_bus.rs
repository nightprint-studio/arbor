//! Frontend event bus wrapper used by plugin namespaces that need to push a
//! string payload under a `plugin:<name>:<event>` topic.
//!
//! Thin wrapper over [`AppCtx::emit`] — the only value-add over calling `emit`
//! directly is the topic-prefix convention. Kept as its own module so the
//! namespace surface (and the future WASM runtime) has a single canonical
//! entry point.

use arbor_core::prelude::AppCtx;

/// Emit a frontend event from a plugin.
///
/// `event_name` is namespaced under `plugin:<plugin_name>:` so a plugin can
/// never collide with built-in `arbor://...` topics.
#[allow(dead_code)]
pub fn emit(ctx: &dyn AppCtx, plugin_name: &str, event_name: &str, payload: &str) {
    let full_event = format!("plugin:{plugin_name}:{event_name}");
    ctx.emit(&full_event, serde_json::Value::String(payload.to_string()));
}
