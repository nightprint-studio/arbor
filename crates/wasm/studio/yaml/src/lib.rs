//! `arbor-studio-yaml` — the Studio YAML backend, extracted from the
//! launcher's `src-tauri/src/yaml_studio/`.
//!
//! YAML is a "simple" format (no variant tags, single editable doc model
//! per stream item), so it rides on
//! [`arbor_studio_core::prelude::DefaultBackend`] via the
//! [`simple::YamlFormat`] impl of [`arbor_studio_core::prelude::SimpleFormat`].
//! `DefaultBackend` owns the doc registry, `History<String>`, encoding
//! round-trip, diff/query/refactor delegation; this crate supplies only the
//! YAML-specific primitives: parse → `yaml_edit::Document` per stream item
//! + `serde_yaml_ng` projection to `serde_json::Value` (multi-doc `---`
//! streams preserved), the structured mutation lowering (lossless
//! `set_path` for scalars, `serde_yaml_ng` round-trip for structural ops),
//! kind/preview, and the capability descriptor.
//!
//! Schema support (YAML declares only JSON Schema) is injected by the
//! caller as a [`arbor_studio_core::prelude::SchemaRouting::Single`] — the
//! crate names no other format crate (the DAG forbids format↔format deps;
//! the launcher/api layer wires the provider).
//!
//! Project-wide F12 rename-preview, project-wide F13 bulk-preview and
//! `list_files` need the repo scanner / `StudioIndex`, which the CALLER
//! orchestrates (`DefaultBackend` returns `Unsupported` for those). The
//! active-doc bulk path and the FS-only `rename_apply` ARE implemented by
//! `DefaultBackend`.

pub mod descriptor;
pub mod kind;
pub mod mutate;
pub mod project;
pub mod refactor_ops;
pub mod simple;

pub mod prelude;

use std::sync::Arc;

use arbor_studio_core::prelude::{DefaultBackend, SchemaRouting, StudioFormatBackend};

use crate::simple::YamlFormat;

/// Build the YAML `StudioFormatBackend`.
///
/// `schema` injects the schema provider: YAML declares only JSON Schema, so
/// the caller passes [`SchemaRouting::Single`] with a JSON adapter; pass
/// [`SchemaRouting::None`] to disable the schema panel. `dedup` is fixed
/// `false` (matches the pre-extraction `History::new`).
pub fn backend_with_schema(schema: SchemaRouting) -> Arc<dyn StudioFormatBackend> {
    Arc::new(DefaultBackend::new(YamlFormat::new(), schema, false))
}

/// Build the YAML backend with no schema provider wired.
///
/// Schema-panel calls then surface `Unsupported`. The launcher uses
/// [`backend_with_schema`] with its JSON adapter; this no-schema variant
/// exists for tests + future callers that don't need schema.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    backend_with_schema(SchemaRouting::None)
}

/// Parse `text` as a YAML stream and project it to `serde_json::Value`.
///
/// Used by the cross-ref scanner (`scan_*` in the launcher / `arbor-studio-api`
/// after Stage 4) to walk YAML alongside the other formats. Returns `None`
/// on parse error — best-effort, matching the scanner's policy. Multi-doc
/// streams project to a `Value::Array`.
pub fn parse_to_value(text: &str) -> Option<serde_json::Value> {
    project::parse_to_value(text)
}
