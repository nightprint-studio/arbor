//! [`DbProviderRegistry`] — engine → `Arc<dyn DbProvider>` lookup.
//!
//! The registry matters as much as the trait. Adding an engine has to be
//! *registering an implementation*; the moment a `match kind { … }` appears in a
//! handler, the next engine costs an edit in every one of them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::descriptor::DbProviderDescriptor;
use crate::error::{DbError, DbResult};
use crate::kind::EngineKind;
use crate::provider::DbProvider;

/// In-memory map keyed by engine.
#[derive(Default, Clone)]
pub struct DbProviderRegistry {
    by_kind: BTreeMap<EngineKind, Arc<dyn DbProvider>>,
}

impl DbProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace the provider for `provider.kind()`.
    pub fn register(&mut self, provider: Arc<dyn DbProvider>) {
        self.by_kind.insert(provider.kind(), provider);
    }

    /// Builder form, for a `let registry = …` at boot.
    pub fn with(mut self, provider: Arc<dyn DbProvider>) -> Self {
        self.register(provider);
        self
    }

    /// The provider for an engine, or `None`.
    pub fn get(&self, kind: EngineKind) -> Option<Arc<dyn DbProvider>> {
        self.by_kind.get(&kind).cloned()
    }

    /// The provider for an engine, or the honest error saying Picus handles that
    /// engine's *scripts* but cannot open a session to it.
    pub fn require(&self, kind: EngineKind) -> DbResult<Arc<dyn DbProvider>> {
        self.get(kind).ok_or_else(|| DbError::NoDriver { engine: kind.to_string() })
    }

    /// Every registered provider's descriptor, in engine order — the payload the
    /// frontend's connection form and schema tree read.
    pub fn descriptors(&self) -> Vec<DbProviderDescriptor> {
        self.by_kind.values().map(|p| p.descriptor()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.by_kind.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_kind.len()
    }
}
