//! `StudioRegistry` — keyed lookup of `Arc<dyn StudioFormatBackend>`
//! by `format_id`. Mounted in `AppState.studio_registry` and populated
//! once at app startup via [`studio_registry`].
//!
//! Registry is immutable after init: backends register at boot and
//! are never swapped at runtime. Interior mutability for per-doc
//! state is each backend's concern (typically a `Mutex` it owns).

use std::collections::HashMap;
use std::sync::Arc;

use arbor_studio_core::prelude::StudioFormatBackend;
use arbor_studio_types::prelude::{FormatDescriptor, StudioError, StudioResult};

pub struct StudioRegistry {
    backends: HashMap<String, Arc<dyn StudioFormatBackend>>,
}

impl StudioRegistry {
    pub fn new() -> Self {
        Self { backends: HashMap::new() }
    }

    pub fn register(&mut self, backend: Arc<dyn StudioFormatBackend>) {
        let id = backend.descriptor().id.clone();
        self.backends.insert(id, backend);
    }

    pub fn get(&self, format_id: &str) -> StudioResult<Arc<dyn StudioFormatBackend>> {
        self.backends
            .get(format_id)
            .cloned()
            .ok_or_else(|| StudioError::UnknownFormat(format_id.to_string()))
    }

    pub fn list_descriptors(&self) -> Vec<FormatDescriptor> {
        let mut out: Vec<FormatDescriptor> = self
            .backends
            .values()
            .map(|b| b.descriptor().clone())
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}

impl Default for StudioRegistry {
    fn default() -> Self { Self::new() }
}

/// Construct the fully-wired Studio registry — all five format backends
/// with their `SchemaProvider` routing + cross-ref index providers
/// injected. This is the single place the launcher's `AppState` calls;
/// the per-backend wiring (which used to live inline in `AppState::new`)
/// is now consolidated here.
pub fn studio_registry() -> StudioRegistry {
    use crate::index_provider::{LauncherJsonIndex, LauncherRonIndex};
    use crate::schema_adapter::{json_only, rust_or_json};

    let mut reg = StudioRegistry::new();
    // RON — hand-written tag-preserving AST + RON-special diff/query/
    // refactor + syn `.rs` schema. Self-serving F12/F13 + list_files run
    // against the api repo scanner / cross-ref index via the injected
    // `LauncherRonIndex` provider.
    reg.register(arbor_studio_ron::backend_with_index(
        Arc::new(LauncherRonIndex),
    ));
    // JSON — hand-written dual parser. Self-serving F12/F13 + list_files
    // run against the api repo scanner / index via `LauncherJsonIndex`.
    reg.register(arbor_studio_json::backend_with_index(
        Arc::new(LauncherJsonIndex),
    ));
    // TOML — DefaultBackend + SimpleFormat. Schema panel = Rust(.rs) +
    // JSON, routed via the api schema adapter.
    reg.register(arbor_studio_toml::backend_with_schema(rust_or_json()));
    // YAML — DefaultBackend + SimpleFormat. Schema panel = JSON only.
    reg.register(arbor_studio_yaml::backend_with_schema(json_only()));
    // .properties — DefaultBackend + SimpleFormat for doc/history/
    // mutation; SPECIAL hand-written F12/F13 routed through the api
    // `project_refactor` module. Schema panel = JSON only.
    reg.register(arbor_studio_properties::backend_with_schema(json_only()));
    reg
}
