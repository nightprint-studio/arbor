//! [`JsonSchemaProvider`] — the real `core::SchemaProvider` for JSON
//! Schema sources, built on the in-crate [`crate::schema`] loader.
//!
//! The simple-format crates (TOML / YAML / .properties) accept JSON
//! Schema sources but can't name this crate (the DAG forbids
//! format→format deps). The `arbor-studio-api` registry injects this
//! provider into each `DefaultBackend` so those formats route their
//! JSON-Schema panel here. JSON's own backend serves its schema methods
//! directly from `crate::schema` (no injection needed for itself).

use async_trait::async_trait;

use arbor_studio_core::prelude::SchemaProvider;
use arbor_studio_types::prelude::{CrateProbe, Schema, StudioResult, TypeSource};

/// `SchemaProvider` backed by the JSON Schema loader (`crate::schema`).
pub struct JsonSchemaProvider;

#[async_trait]
impl SchemaProvider for JsonSchemaProvider {
    async fn probe(&self, source: &str) -> StudioResult<CrateProbe> {
        crate::schema::probe(source)
    }
    async fn load(&self, source: &str, root_canonical: &str) -> StudioResult<Schema> {
        crate::schema::load(source, root_canonical)
    }
    async fn view_source(&self, source: &str, canonical: &str) -> StudioResult<TypeSource> {
        crate::schema::get_type_source(source, canonical)
    }
}
