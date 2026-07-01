//! `dispatch` — registry-level routing facade for the stateless Studio
//! methods that need only a [`StudioRegistry`] (no tab-path resolution,
//! no Tauri `AppState`, no async IO).
//!
//! The launcher's `#[studio::handler(program = "studio")]` modules remain
//! the canonical IPC seam (they own tab-path resolution, the event sink,
//! `spawn_blocking`, and the async runtime). This `dispatch` is the
//! Tauri-free entry point that those handlers — or a future `studio-be`
//! binary — can delegate the registry-only verbs to, keeping the routing
//! table in one place. Methods requiring repo/tab context are NOT routed
//! here; they call the discrete `crate::{scanner, index, project_refactor}`
//! functions directly.

use arbor_studio_types::prelude::{StudioError, StudioResult};
use serde_json::Value;

use crate::registry::StudioRegistry;

/// Route a registry-level method to the registry. `params` is the decoded
/// JSON argument object; the return is the method's JSON-serialised result.
///
/// Recognised methods:
///   · `list_descriptors` / `studio_list_formats` — every backend's
///     `FormatDescriptor`, sorted by id.
///   · `describe` / `studio_describe` — a single backend's descriptor
///     (`{ "format_id": "ron" }`).
///
/// Unknown / context-requiring methods surface `StudioError::App` so the
/// caller knows to route them through the discrete functions instead.
pub fn dispatch(reg: &StudioRegistry, method: &str, params: &Value) -> StudioResult<Value> {
    match method {
        "list_descriptors" | "studio_list_formats" => {
            serde_json::to_value(reg.list_descriptors())
                .map_err(|e| StudioError::App(format!("encode list_descriptors: {e}")))
        }
        "describe" | "studio_describe" => {
            let format_id = params
                .get("format_id")
                .and_then(Value::as_str)
                .ok_or_else(|| StudioError::App("describe: missing `format_id`".into()))?;
            let backend = reg.get(format_id)?;
            serde_json::to_value(backend.descriptor().clone())
                .map_err(|e| StudioError::App(format!("encode describe: {e}")))
        }
        other => Err(StudioError::App(format!(
            "studio dispatch: method `{other}` needs repo/tab context — route it through the discrete api functions"
        ))),
    }
}
