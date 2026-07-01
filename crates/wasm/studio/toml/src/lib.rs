//! `arbor-studio-toml` — the Studio TOML backend, extracted from the
//! launcher's `src-tauri/src/toml_studio/`.
//!
//! TOML is a "simple" format (no variant tags, single editable doc model),
//! so it rides on [`arbor_studio_core::prelude::DefaultBackend`] via the
//! [`simple::TomlFormat`] impl of [`arbor_studio_core::prelude::SimpleFormat`].
//! `DefaultBackend` owns the doc registry, `History<String>`, encoding
//! round-trip, diff/query/refactor delegation; this crate supplies only the
//! TOML-specific primitives: parse → `toml_edit::DocumentMut` → project to
//! `serde_json::Value`, the structured mutation lowering, kind/preview, and
//! the capability descriptor.
//!
//! Schema support (TOML declares both `.rs` (Rust struct) and JSON Schema
//! sources) is injected by the caller as a
//! [`arbor_studio_core::prelude::SchemaRouting`] — the crate names no other
//! format crate (the DAG forbids format↔format deps; the launcher/api layer
//! wires the providers).
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

use crate::simple::TomlFormat;

/// Build the TOML `StudioFormatBackend`.
///
/// `schema` injects the schema provider(s): TOML declares both Rust + JSON
/// sources, so the caller passes [`SchemaRouting::RustOrOther`]; pass
/// [`SchemaRouting::None`] to disable the schema panel. `dedup` is the
/// history coalesce flag — `false` for TOML (matches the pre-extraction
/// `History::new`).
pub fn backend_with_schema(schema: SchemaRouting) -> Arc<dyn StudioFormatBackend> {
    Arc::new(DefaultBackend::new(TomlFormat::new(), schema, false))
}

/// Build the TOML backend with no schema provider wired.
///
/// Schema-panel calls then surface `Unsupported`. The launcher uses
/// [`backend_with_schema`] with its Rust+JSON adapters; this no-schema
/// variant exists for tests + future callers that don't need schema.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    backend_with_schema(SchemaRouting::None)
}

/// Parse `text` as a TOML document and project it to `serde_json::Value`.
///
/// Used by the cross-ref scanner (`scan_*` in the launcher / `arbor-studio-api`
/// after Stage 4) to walk TOML alongside the other formats. Returns `None`
/// on parse error — best-effort, matching the scanner's policy.
pub fn parse_to_value(text: &str) -> Option<serde_json::Value> {
    project::parse_to_value(text)
}
