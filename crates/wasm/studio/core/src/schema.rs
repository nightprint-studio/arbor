//! `core::schema` — the `SchemaProvider` seam (blueprint §3.5).
//!
//! Today TOML / YAML / .properties `backend_impl.rs` reach directly into
//! `crate::json_studio::schema::{probe,load,get_type_source}` and
//! `crate::ron_studio::schema::*` — a **format→format dependency** the
//! extracted crate DAG forbids (`ron`, `json`, `toml`, `yaml`,
//! `properties` may depend on `core` + `types`, never on each other).
//!
//! `SchemaProvider` breaks that cycle: `arbor-studio-ron` will expose an
//! `RsSchemaProvider` (the syn `.rs` loader), `arbor-studio-json` a
//! `JsonSchemaProvider` (the JSON-Schema loader), and the
//! `arbor-studio-api` registry **injects** the right provider(s) into
//! each [`crate::backend::DefaultBackend`] at construction (TOML gets
//! both Rust + JSON, YAML / .properties get JSON only). The format
//! crates no longer name each other — they hold an
//! `Arc<dyn SchemaProvider>` instead.
//!
//! The three trait methods are the exact shapes the `StudioFormatBackend`
//! schema methods forward to, returning the plain `types` DTOs the FE
//! schema panel consumes.

use async_trait::async_trait;

use arbor_studio_types::prelude::{CrateProbe, Schema, StudioResult, TypeSource};

/// Loads + walks a schema source (a `.rs` crate via syn, or a JSON
/// Schema file) into the shared schema-view DTOs. Implemented in the
/// `ron` / `json` format crates; injected per-format by the registry.
#[async_trait]
pub trait SchemaProvider: Send + Sync {
    /// Probe a schema `source` for its root candidates (the dropdown the
    /// FE populates before the user picks a root type).
    async fn probe(&self, source: &str) -> StudioResult<CrateProbe>;

    /// Load the full resolved [`Schema`] rooted at `root_canonical`.
    async fn load(&self, source: &str, root_canonical: &str) -> StudioResult<Schema>;

    /// Return the source text of one resolved type (`canonical` path) for
    /// the "view source" affordance.
    async fn view_source(&self, source: &str, canonical: &str) -> StudioResult<TypeSource>;
}
