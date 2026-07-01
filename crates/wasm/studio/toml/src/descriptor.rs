//! The TOML capability descriptor (hard-coded capability matrix), lifted
//! from the launcher's `toml_studio/backend_impl.rs::build_descriptor`.

use std::collections::BTreeMap;

use arbor_studio_types::prelude::{
    CrossRefScope, FormatDescriptor, IconRef, KindStyle, KindTone, NullPolicy, QuerySyntax,
    SchemaSourceKind,
};

/// Build the TOML `FormatDescriptor`.
pub fn build_descriptor() -> FormatDescriptor {
    let mut kind_palette = BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    // FROZEN F11 — TOML kinds stay distinct (table vs inline_table vs
    // array_of_tables, etc.). The FE renders each one with its own chip.
    kind_palette.insert("table".into(),           entry("table",       KindTone::Info));
    kind_palette.insert("inline_table".into(),    entry("inline",      KindTone::Info));
    kind_palette.insert("array".into(),           entry("array",       KindTone::Info));
    kind_palette.insert("array_of_tables".into(), entry("array<table>", KindTone::Accent));
    kind_palette.insert("string".into(),          entry("string",      KindTone::Success));
    kind_palette.insert("integer".into(),         entry("int",         KindTone::Warning));
    kind_palette.insert("float".into(),           entry("float",       KindTone::Warning));
    kind_palette.insert("bool".into(),            entry("bool",        KindTone::Warning));
    kind_palette.insert("datetime".into(),        entry("datetime",    KindTone::Accent));

    FormatDescriptor {
        id:              "toml".into(),
        label:           "TOML".into(),
        file_extensions: vec![".toml".into()],
        icon:            IconRef::Iconify {
            name: "vscode-icons:file-type-toml".into(),
        },

        // `toml_edit` preserves formatting natively so mutations round-trip
        // losslessly.
        supports_lossless_edit: true,
        supports_comments:      true,
        supports_anchors:       false,
        // FROZEN F13 — TOML has no native null; bulk-edit `null` → remove
        // key. The descriptor drives the modal banner so the FE doesn't
        // branch on `format_id`.
        null_handling: NullPolicy::AsDelete,

        // Streaming mode: not wired for TOML.
        supports_streaming_mode: false,
        streaming_threshold_kb:  None,
        streaming_setting_key:   None,

        query_syntax: QuerySyntax::JsonPath,

        // Default convention — same as RON / JSON.
        cross_ref_default_fields: vec!["id".into(), "name".into()],
        cross_ref_scopes:         vec![CrossRefScope::Value],

        schema_sources: vec![SchemaSourceKind::RustStruct, SchemaSourceKind::JsonSchema],

        kind_palette,

        save_warnings:             Vec::new(),
        save_behavior_setting_key: None,

        // TOML projection to JSON is meaningful (schema panel + conversions).
        convert_to_json_supported: true,

        supports_external_files: true,

        // F12 rename + F13 bulk edit lit up.
        supports_rename_reference: true,
        supports_bulk_edit:        true,
    }
}
