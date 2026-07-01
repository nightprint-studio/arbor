//! Canonical entry point for `arbor-studio-properties`'s public API.
//!
//! Call sites reach this crate through `arbor_studio_properties::prelude::...`.

pub use crate::{backend, backend_with_schema, collect_kv_pairs, parse_to_value};
pub use crate::simple::PropertiesFormat;

// The SPECIAL hand-written F12/F13 refactor surface the launcher drives.
pub use crate::refactor::{
    build_ops_from_sites, build_site_for_preview, compute_new_value, render_set_preview,
    synth_active_doc_paths, synth_preview_line, PropertiesRefactor,
};

// Line-model F12/F13 transform types the launcher orchestrator hands to
// the apply path.
pub use crate::line_model::{
    apply_bulk_edits_text, apply_rename_in_text, PropertiesBulkOp, PropertiesRenameScope,
    PropertiesRenameSite, PropertiesSetValue,
};

// The bundled YAML ↔ .properties converter (repointed convert IPC handler).
pub use crate::codec::{
    properties_to_yaml, yaml_to_properties, PropertiesToYamlOptions, PropertiesToYamlOutput,
    YamlToPropertiesOutput,
};

// Re-export the schema-routing knob the caller constructs the backend with.
pub use arbor_studio_core::prelude::SchemaRouting;
