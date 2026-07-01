//! Serde shape tests for the discriminant-bearing DTOs.
//!
//! These lock the `#[serde(tag/rename_all)]` wire shape the FE depends
//! on: a rename slip here would silently mis-route a mutation or disable
//! an FE affordance. We test the direction each type actually travels —
//! `StudioMutation` only deserializes (FE → BE); the diff/descriptor
//! shapes only serialize (BE → FE); `BulkEditValueSource` does both.

use std::collections::BTreeMap;

use serde_json::json;

use crate::descriptor::*;
use crate::dto::*;

#[test]
fn studio_mutation_deserializes_each_variant_by_tag() {
    let set: StudioMutation =
        serde_json::from_value(json!({ "kind": "set_primitive", "path": ["a", "0"], "value": 42 }))
            .expect("set_primitive");
    assert!(matches!(set, StudioMutation::SetPrimitive { .. }));

    let toggle: StudioMutation =
        serde_json::from_value(json!({ "kind": "toggle_option", "path": ["a"] }))
            .expect("toggle_option");
    assert!(matches!(toggle, StudioMutation::ToggleOption { .. }));

    let mv: StudioMutation =
        serde_json::from_value(json!({ "kind": "move_item", "path": ["a"], "delta": -1 }))
            .expect("move_item");
    match mv {
        StudioMutation::MoveItem { delta, .. } => assert_eq!(delta, -1),
        _ => panic!("expected move_item"),
    }

    let entry: StudioMutation = serde_json::from_value(json!({
        "kind": "insert_map_entry", "path": [], "key_text": "k", "val_text": "v"
    }))
    .expect("insert_map_entry");
    assert!(matches!(entry, StudioMutation::InsertMapEntry { .. }));
}

#[test]
fn bulk_edit_value_source_round_trips() {
    // Literal arm.
    let lit = BulkEditValueSource::Literal {
        literal: BulkEditLiteral::Number(3.5),
    };
    let v = serde_json::to_value(&lit).unwrap();
    assert_eq!(v["kind"], "literal");
    assert_eq!(v["literal"]["type"], "number");
    assert_eq!(v["literal"]["value"], 3.5);
    let back: BulkEditValueSource = serde_json::from_value(v).unwrap();
    match back {
        BulkEditValueSource::Literal { literal: BulkEditLiteral::Number(n) } => {
            assert_eq!(n, 3.5)
        }
        _ => panic!("expected literal/number"),
    }

    // Expression arm.
    let expr = BulkEditValueSource::Expression { source: "old + 1".into() };
    let v = serde_json::to_value(&expr).unwrap();
    assert_eq!(v["kind"], "expression");
    assert_eq!(v["source"], "old + 1");
    let back: BulkEditValueSource = serde_json::from_value(v).unwrap();
    assert!(matches!(back, BulkEditValueSource::Expression { .. }));
}

#[test]
fn bulk_edit_literal_null_and_bool_round_trip() {
    for (lit, ty) in [
        (BulkEditLiteral::Null, "null"),
        (BulkEditLiteral::Bool(true), "bool"),
        (BulkEditLiteral::String("x".into()), "string"),
    ] {
        let v = serde_json::to_value(&lit).unwrap();
        assert_eq!(v["type"], ty);
        let back: BulkEditLiteral = serde_json::from_value(v).unwrap();
        assert_eq!(back, lit);
    }
}

#[test]
fn diff_status_serializes_snake_case() {
    assert_eq!(serde_json::to_value(DiffStatus::Unchanged).unwrap(), "unchanged");
    assert_eq!(serde_json::to_value(DiffStatus::Added).unwrap(), "added");
    assert_eq!(serde_json::to_value(DiffStatus::Removed).unwrap(), "removed");
    assert_eq!(serde_json::to_value(DiffStatus::Modified).unwrap(), "modified");
    assert_eq!(serde_json::to_value(DiffStatus::Partial).unwrap(), "partial");
}

#[test]
fn diff_tree_node_serializes_expected_shape() {
    let node = DiffTreeNode {
        key: "root".into(),
        path: vec![],
        status: DiffStatus::Partial,
        kind_before: Some("object".into()),
        kind_after: Some("object".into()),
        preview_before: None,
        preview_after: None,
        tag_before: None,
        tag_after: None,
        children: vec![DiffTreeNode {
            key: "a".into(),
            path: vec!["a".into()],
            status: DiffStatus::Modified,
            kind_before: Some("int".into()),
            kind_after: Some("int".into()),
            preview_before: Some("1".into()),
            preview_after: Some("2".into()),
            tag_before: None,
            tag_after: None,
            children: vec![],
            change_count: 1,
        }],
        change_count: 1,
    };
    let v = serde_json::to_value(&node).unwrap();
    assert_eq!(v["status"], "partial");
    assert_eq!(v["change_count"], 1);
    assert_eq!(v["children"][0]["status"], "modified");
    assert_eq!(v["children"][0]["preview_after"], "2");
}

#[test]
fn rename_site_scope_round_trips() {
    for (scope, wire) in [
        (RenameSiteScope::Definition, "definition"),
        (RenameSiteScope::Reference, "reference"),
        (RenameSiteScope::Key, "key"),
    ] {
        let v = serde_json::to_value(scope).unwrap();
        assert_eq!(v, wire);
        let back: RenameSiteScope = serde_json::from_value(v).unwrap();
        assert_eq!(back, scope);
    }
}

#[test]
fn format_descriptor_exposes_capability_flag_field_names() {
    let desc = FormatDescriptor {
        id: "json".into(),
        label: "JSON".into(),
        file_extensions: vec!["json".into()],
        icon: IconRef::Iconify { name: "vscode-icons:file-type-json".into() },
        supports_lossless_edit: true,
        supports_comments: false,
        supports_anchors: false,
        null_handling: NullPolicy::Native,
        supports_streaming_mode: true,
        streaming_threshold_kb: Some(1024),
        streaming_setting_key: None,
        query_syntax: QuerySyntax::JsonPath,
        cross_ref_default_fields: vec!["id".into()],
        cross_ref_scopes: vec![CrossRefScope::Value],
        schema_sources: vec![SchemaSourceKind::JsonSchema],
        kind_palette: {
            let mut m = BTreeMap::new();
            m.insert(
                "object".into(),
                KindStyle { label: "Object".into(), tone: KindTone::Info, icon: None },
            );
            m
        },
        save_warnings: vec![SaveWarningKind::JsoncCommentsInJson],
        save_behavior_setting_key: None,
        convert_to_json_supported: false,
        supports_external_files: true,
        supports_rename_reference: true,
        supports_bulk_edit: true,
    };
    let v = serde_json::to_value(&desc).unwrap();
    // The exact field names the FE gates affordances on.
    for flag in [
        "supports_lossless_edit",
        "supports_comments",
        "supports_anchors",
        "supports_streaming_mode",
        "supports_external_files",
        "supports_rename_reference",
        "supports_bulk_edit",
        "convert_to_json_supported",
        "null_handling",
        "query_syntax",
        "kind_palette",
    ] {
        assert!(v.get(flag).is_some(), "descriptor missing field `{flag}`");
    }
    assert_eq!(v["supports_rename_reference"], true);
    assert_eq!(v["supports_bulk_edit"], true);
    assert_eq!(v["null_handling"], "native");
    assert_eq!(v["query_syntax"], "json_path");
    assert_eq!(v["icon"]["type"], "iconify");
}
