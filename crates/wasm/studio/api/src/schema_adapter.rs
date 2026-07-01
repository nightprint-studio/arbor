//! `SchemaProvider` routing for the simple-format crates
//! (`arbor-studio-toml` / `arbor-studio-yaml` / `arbor-studio-properties`).
//!
//! The extracted crates can't name each other (the crate DAG forbids
//! format->format deps), so they hold an
//! `Arc<dyn arbor_studio_core::prelude::SchemaProvider>` injected by the
//! api registry. Both providers are the real extracted-crate ones:
//!
//!   - Rust `.rs` schema -> `arbor_studio_ron::RsSchemaProvider`
//!     (syn-based crate walking).
//!   - JSON Schema -> `arbor_studio_json::JsonSchemaProvider`.
//!
//! Reuse: `toml` routes `.rs` -> Rust, everything else -> JSON via
//! [`rust_or_json`]; the JSON-only formats (yaml / .properties) take
//! [`json_only`] (a single JSON provider) wrapped in
//! `SchemaRouting::Single`.

use std::sync::Arc;

use arbor_studio_core::prelude::SchemaRouting;
use arbor_studio_json::prelude::JsonSchemaProvider;
use arbor_studio_ron::prelude::RsSchemaProvider;

/// TOML routing: `.rs` -> Rust crate walker (`arbor-studio-ron`),
/// everything else -> JSON Schema (`arbor-studio-json`).
pub fn rust_or_json() -> SchemaRouting {
    SchemaRouting::RustOrOther {
        rust:  Arc::new(RsSchemaProvider),
        other: Arc::new(JsonSchemaProvider),
    }
}

/// JSON-only routing (yaml / .properties): every source goes to the
/// extracted `arbor-studio-json` JSON Schema provider.
pub fn json_only() -> SchemaRouting {
    SchemaRouting::Single(Arc::new(JsonSchemaProvider))
}
