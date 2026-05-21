//! `StudioRegistry` — keyed lookup of `Arc<dyn StudioFormatBackend>`
//! by `format_id`. Mounted in `AppState.studio_registry` and populated
//! once at app startup.
//!
//! Registry is immutable after init: backends register at boot and
//! are never swapped at runtime. Interior mutability for per-doc
//! state is each backend's concern (typically a `Mutex` it owns).

use std::collections::HashMap;
use std::sync::Arc;

use super::backend::StudioFormatBackend;
use super::descriptor::FormatDescriptor;
use super::errors::{StudioError, StudioResult};

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
