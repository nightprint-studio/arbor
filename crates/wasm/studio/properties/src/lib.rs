//! `arbor-studio-properties` — the Studio `.properties` backend,
//! extracted from the launcher's `src-tauri/src/properties_studio/`.
//!
//! `.properties` is a "simple" format for the doc/history/mutation side,
//! so it rides on [`arbor_studio_core::prelude::DefaultBackend`] via the
//! [`simple::PropertiesFormat`] impl of
//! [`arbor_studio_core::prelude::SimpleFormat`] — with **dedup history ON**
//! (`DefaultBackend::new(.., dedup=true)`): a replayed identical snapshot
//! is a no-op. `DefaultBackend` owns the doc registry, `History<String>`,
//! encoding round-trip, diff/query delegation; this crate supplies the
//! `.properties`-specific primitives: the byte-preserving line model
//! ([`line_model`]) with continuation/escape/`\uXXXX` handling, the JSON
//! projection with the `$value` prefix-collision sentinel ([`project`]),
//! the structured mutation lowering ([`mutate`]), and the capability
//! descriptor ([`descriptor`]).
//!
//! ## The SPECIAL divergence: F12/F13 stay hand-written
//!
//! `.properties` does NOT route F12/F13 through `DefaultBackend`'s default
//! `RefactorOps`:
//! * **F12 is key-scoped** — it renames the dotted *key* (per-site `Key` /
//!   `Value` scope + an `old_value`), which `DefaultBackend`'s
//!   string-leaf `apply_string_rename` can't express.
//! * **F13 coerces every value to a string with an `(empty)` sentinel**
//!   and renders a divergent preview.
//!
//! So the launcher routes properties F12/F13 through [`refactor::PropertiesRefactor`]
//! (this crate) + `studio/project_refactor.rs` (the launcher's scan/index
//! orchestration), bypassing the backend's `rename_apply` / project-wide
//! bulk paths. The active-doc bulk + the doc/history/mutation/diff/query
//! side DO ride `DefaultBackend`.
//!
//! Schema support (`.properties` declares only JSON Schema) is injected by
//! the caller as [`arbor_studio_core::prelude::SchemaRouting::Single`].
//!
//! The YAML ↔ `.properties` converter ([`codec`]) is bundled here (it is a
//! `.properties`-specific cross-format primitive) — this is what lets
//! `serde_yaml_ng` + `yaml-edit` leave `src-tauri/Cargo.toml`.

pub mod codec;
pub mod descriptor;
pub mod line_model;
pub mod mutate;
pub mod project;
pub mod refactor;
pub mod simple;

pub mod prelude;

use std::sync::Arc;

use arbor_studio_core::prelude::{DefaultBackend, SchemaRouting, StudioFormatBackend};

use crate::simple::PropertiesFormat;

/// Build the `.properties` `StudioFormatBackend`.
///
/// `schema` injects the schema provider: `.properties` declares only JSON
/// Schema, so the caller passes [`SchemaRouting::Single`] with a JSON
/// adapter; pass [`SchemaRouting::None`] to disable the schema panel.
/// `dedup` is fixed `true` (the SPECIAL divergence: `.properties` opts
/// into no-op snapshot suppression — matches the pre-extraction
/// `History::new_dedup`).
pub fn backend_with_schema(schema: SchemaRouting) -> Arc<dyn StudioFormatBackend> {
    Arc::new(DefaultBackend::new(PropertiesFormat::new(), schema, true))
}

/// Build the `.properties` backend with no schema provider wired.
///
/// Schema-panel calls then surface `Unsupported`. The launcher uses
/// [`backend_with_schema`] with its JSON adapter; this no-schema variant
/// exists for tests + future callers that don't need schema.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    backend_with_schema(SchemaRouting::None)
}

/// Parse `text` as `.properties` and project it to `serde_json::Value`.
///
/// Used by the cross-ref scanner to walk `.properties` alongside the other
/// formats. Returns `None` on parse error (best-effort).
pub fn parse_to_value(text: &str) -> Option<serde_json::Value> {
    project::parse_to_value(text)
}

/// Walk every logical key in the document and return `(key, value)`
/// pairs. Used by the project-wide def / usage / broken-ref scanners
/// (FROZEN F5: every key in `.properties` is a potential cross-ref
/// target, every string value a potential reference).
pub fn collect_kv_pairs(text: &str) -> Vec<(String, String)> {
    line_model::collect_kv_pairs(text)
}
