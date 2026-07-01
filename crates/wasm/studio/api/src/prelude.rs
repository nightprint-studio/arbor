//! Canonical entry point for `arbor-studio-api`'s public API.
//!
//! Call sites reach this crate through `arbor_studio_api::prelude::...`.

pub use crate::dispatch::dispatch;
pub use crate::registry::{studio_registry, StudioRegistry};

// Scanner surface — the file walk + cross-ref / usage / broken-ref scans
// and their DTOs.
pub use crate::scanner::{
    find_usages_for, scan_broken_refs_for, scan_cross_refs_for, scan_repo, BrokenRef, CrossRefDef,
    EntrySchema, SchemaHintOriginExt, StudioFileEntry, StudioFileKind, UsageMatch,
};

// Persistent index surface.
pub use crate::index::{
    self, aggregate_broken_refs_for, aggregate_cross_refs_for, aggregate_usages_for, IndexedDef,
    IndexedFile, IndexedRef, ProgressFn, StudioIndex,
};

// Repo-root config surface.
pub use crate::config::{self, StudioConfig};

// Project-wide F12/F13 orchestration.
pub use crate::project_refactor;

// The YAML ↔ .properties converter (lives in `arbor-studio-properties`).
// Re-exported so the launcher's IPC seam reaches it through api without
// naming the format crate directly.
pub use arbor_studio_properties::prelude::{
    properties_to_yaml, yaml_to_properties, PropertiesToYamlOptions, PropertiesToYamlOutput,
    YamlToPropertiesOutput,
};

// Re-export the studio DTO + error surface for convenience.
pub use arbor_studio_types::prelude::*;
