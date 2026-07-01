//! The `.properties` capability descriptor (hard-coded capability
//! matrix), lifted from the launcher's
//! `properties_studio/backend_impl.rs::build_descriptor`.

use std::collections::BTreeMap;

use arbor_studio_types::prelude::{
    CrossRefScope, FormatDescriptor, IconRef, KindStyle, KindTone, NullPolicy, QuerySyntax,
    SchemaSourceKind,
};

/// Inline SVG glyph for `.properties` (FROZEN F8 fallback). Two
/// "key=value" rows on a rounded doc background. Embedded into the
/// descriptor so the FE renders it via the same `IconRef` path as the
/// Iconify-backed glyphs.
const PROPERTIES_INLINE_SVG: &str = r##"<svg viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg"><rect x="3" y="3" width="18" height="18" rx="2" fill="#37474F"/><path d="M6 9h4M11 9h7M6 13h3M10 13h8M6 17h5M12 17h6" stroke="#FFD54F" stroke-width="1.5" stroke-linecap="round"/></svg>"##;

/// Build the `.properties` `FormatDescriptor`.
pub fn build_descriptor() -> FormatDescriptor {
    let mut kind_palette = BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    kind_palette.insert("object".into(), entry("object", KindTone::Info));
    kind_palette.insert("array".into(),  entry("array",  KindTone::Info));
    kind_palette.insert("string".into(), entry("string", KindTone::Success));
    kind_palette.insert("null".into(),   entry("null",   KindTone::Muted));

    FormatDescriptor {
        id:                          "properties".into(),
        label:                       ".properties".into(),
        file_extensions:             vec![".properties".into()],
        // FROZEN F8 — vscode-icons doesn't ship a `file-type-properties`
        // glyph in the version we have installed and material-icon-theme
        // is not in the dependency tree. Final fallback: inline SVG.
        // The glyph shows a stylised key=value pair on two lines so the
        // tab/sidebar chip reads as "config / key-value" at a glance.
        icon:                        IconRef::InlineSvg {
            svg: PROPERTIES_INLINE_SVG.into(),
        },

        supports_lossless_edit:      true,
        supports_comments:           true,
        supports_anchors:            false,
        // FROZEN F4 — `.properties` has no native null. The bulk-edit
        // modal surfaces "Set to empty value" as the implicit policy;
        // "Remove key entirely" is reachable via the `Delete` action.
        null_handling:               NullPolicy::AskUser,

        supports_streaming_mode:     false,
        streaming_threshold_kb:      None,
        streaming_setting_key:       None,

        query_syntax:                QuerySyntax::JsonPath,

        // FROZEN F5 — every key is a target, every value is a ref. We
        // expose this as `[Key, Value]` so the rename modal can group
        // sites by scope and the FE can decide which chip to render.
        cross_ref_default_fields:    Vec::new(),
        cross_ref_scopes:            vec![CrossRefScope::Key, CrossRefScope::Value],

        schema_sources:              vec![SchemaSourceKind::JsonSchema],

        kind_palette,

        save_warnings:               vec![],
        save_behavior_setting_key:   None,

        convert_to_json_supported:   false,

        supports_external_files:     true,

        supports_rename_reference:   true,
        supports_bulk_edit:          true,
    }
}
