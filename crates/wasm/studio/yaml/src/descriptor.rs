//! The YAML capability descriptor (hard-coded capability matrix), lifted
//! from the launcher's `yaml_studio/backend_impl.rs::build_descriptor`.

use std::collections::BTreeMap;

use arbor_studio_types::prelude::{
    CrossRefScope, FormatDescriptor, IconRef, KindStyle, KindTone, NullPolicy, QuerySyntax,
    SchemaSourceKind,
};

/// Build the YAML `FormatDescriptor`.
pub fn build_descriptor() -> FormatDescriptor {
    let mut kind_palette = BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    kind_palette.insert("object".into(), entry("object", KindTone::Info));
    kind_palette.insert("array".into(), entry("array", KindTone::Info));
    kind_palette.insert("string".into(), entry("string", KindTone::Success));
    kind_palette.insert("integer".into(), entry("int", KindTone::Warning));
    kind_palette.insert("float".into(), entry("float", KindTone::Warning));
    kind_palette.insert("bool".into(), entry("bool", KindTone::Warning));
    kind_palette.insert("null".into(), entry("null", KindTone::Muted));

    FormatDescriptor {
        id:              "yaml".into(),
        label:           "YAML".into(),
        file_extensions: vec![".yaml".into(), ".yml".into()],
        icon:            IconRef::Iconify {
            name: "vscode-icons:file-type-yaml".into(),
        },

        // Lossless edit via `yaml_edit`. SetPrimitive on a scalar preserves
        // comments + anchors + quote style; structural ops round-trip the
        // affected sub-tree through `serde_yaml_ng` (may drop comments at
        // the splice site) but keep the rest of the doc intact.
        supports_lossless_edit: true,
        supports_comments:      true,
        supports_anchors:       true,
        // YAML has first-class null — keep it literal (unlike TOML's
        // AsDelete).
        null_handling: NullPolicy::Native,

        supports_streaming_mode: false,
        streaming_threshold_kb:  None,
        streaming_setting_key:   None,

        query_syntax: QuerySyntax::JsonPath,

        cross_ref_default_fields: vec!["id".into(), "name".into()],
        cross_ref_scopes:         vec![CrossRefScope::Value],

        // YAML's only declared schema source is JSON Schema (no Rust probe).
        schema_sources: vec![SchemaSourceKind::JsonSchema],

        kind_palette,

        save_warnings:             Vec::new(),
        save_behavior_setting_key: None,

        convert_to_json_supported: true,

        supports_external_files: true,

        // F12 rename + F13 bulk edit lit up.
        supports_rename_reference: true,
        supports_bulk_edit:        true,
    }
}
