//! §6 RON crown-jewel tests — round-trip preservation + the ron-special
//! behaviors (variant-tag preservation, forced float `.0`, tag-aware
//! tree-diff, RON↔JSON structural round-trip). Pure logic against the
//! registry / AST; no Tauri, no FS-destructive ops (the encoding test
//! round-trips through `arbor-fs` in memory, mirroring the save path).

use crate::ast::{self, RonAst};
use crate::registry::{DiffStatus, DiffTreeNode, NodeKind, PrimitiveValue, RonStudioRegistry};

/// Open a doc and return (registry, doc_id).
fn open(text: &str, path: Option<&str>) -> (RonStudioRegistry, String) {
    let mut reg = RonStudioRegistry::default();
    let res = reg
        .parse(
            text.to_string(),
            path.map(|p| p.to_string()),
            "UTF-8".to_string(),
            false,
        )
        .expect("parse");
    (reg, res.doc_id)
}

// ── Variant tags survive a primitive edit + undo ─────────────────────────

#[test]
fn variant_tags_preserved_through_edit_and_undo() {
    // `element: Dark` (unit variant), `weapon: Sword(damage: 10)` (named
    // struct variant), `mode: Action("hit")` (named tuple variant).
    let src = "(\n  element: Dark,\n  weapon: Sword(\n    damage: 10,\n  ),\n  mode: Action(\"hit\"),\n)";
    let (mut reg, id) = open(src, Some("cfg.ron"));

    // Edit the inner damage 10 → 20. The named struct tag `Sword` and the
    // sibling variant tags must survive the pretty-print round-trip.
    let res = reg
        .mutate_primitive(
            &id,
            &["weapon".to_string(), "damage".to_string()],
            PrimitiveValue::Int(20),
        )
        .expect("mutate damage");

    assert!(res.text.contains("element: Dark"), "unit variant tag dropped:\n{}", res.text);
    assert!(res.text.contains("weapon: Sword("), "named struct tag dropped:\n{}", res.text);
    assert!(res.text.contains("mode: Action("), "named tuple tag dropped:\n{}", res.text);
    assert!(res.text.contains("damage: 20"), "edit not applied:\n{}", res.text);

    // The variant kinds are reported through the tree as named_* / unit_variant.
    let root = reg.get_root(&id).expect("root").expect("parsed");
    assert_eq!(root.kind, NodeKind::Struct);
    let children = reg.get_children(&id, &[]).expect("children");
    let element = children.iter().find(|c| c.key == "element").unwrap();
    assert_eq!(element.kind, NodeKind::UnitVariant);
    assert_eq!(element.variant_tag.as_deref(), Some("Dark"));
    let weapon = children.iter().find(|c| c.key == "weapon").unwrap();
    assert_eq!(weapon.kind, NodeKind::NamedStruct);
    assert_eq!(weapon.variant_tag.as_deref(), Some("Sword"));
    let mode = children.iter().find(|c| c.key == "mode").unwrap();
    assert_eq!(mode.kind, NodeKind::NamedTuple);
    assert_eq!(mode.variant_tag.as_deref(), Some("Action"));
}

// ── Undo after a tree mutation restores byte-identical original ───────────

#[test]
fn undo_after_mutation_is_byte_identical() {
    // The original text is itself in canonical pretty form so the first
    // parse + the undo snapshot are comparable byte-for-byte.
    let src = ast::to_pretty_string(
        &ast::parse("(\n  element: Dark,\n  hp: 100,\n)").unwrap(),
    );
    let (mut reg, id) = open(&src, Some("cfg.ron"));

    reg.mutate_primitive(&id, &["hp".to_string()], PrimitiveValue::Int(50))
        .expect("mutate");
    assert_ne!(reg.raw_current(&id).unwrap(), src);

    let undone = reg.undo(&id).expect("undo");
    assert_eq!(undone.text, src, "undo not byte-identical");
    assert_eq!(reg.raw_current(&id).unwrap(), src);
}

// ── Float `.0` is forced on integral floats ──────────────────────────────

#[test]
fn float_disambiguator_is_forced() {
    // A float value that is integral (3.0) must keep a trailing `.0` in
    // the re-emitted text so RON parsers don't read it back as an int.
    let pretty = ast::to_pretty_string(&RonAst::Float(3.0));
    assert_eq!(pretty, "3.0");

    // Inside a struct, an edit installing a float keeps the `.0`.
    let (mut reg, id) = open("(\n  ratio: 1.5,\n)", Some("cfg.ron"));
    let res = reg
        .mutate_primitive(&id, &["ratio".to_string()], PrimitiveValue::Float(2.0))
        .expect("mutate float");
    assert!(res.text.contains("ratio: 2.0"), "float `.0` not forced:\n{}", res.text);

    // A non-integral float keeps its fractional part as-is.
    let pretty2 = ast::to_pretty_string(&RonAst::Float(0.25));
    assert_eq!(pretty2, "0.25");
}

// ── to_json / from_json structural round-trip ────────────────────────────

#[test]
fn ron_json_structural_round_trip() {
    let src = "(\n  name: \"goblin\",\n  hp: 30,\n  speed: 1.5,\n  tags: [\"fast\", \"weak\"],\n)";
    let (reg, id) = open(src, Some("cfg.ron"));

    // RON → JSON: the struct projects to an object (anonymous struct → no
    // `$type`), arrays stay arrays, numbers/strings preserved.
    let json_text = reg.to_json(&id).expect("to_json");
    let j: serde_json::Value = serde_json::from_str(&json_text).expect("valid json");
    assert_eq!(j["name"], serde_json::json!("goblin"));
    assert_eq!(j["hp"], serde_json::json!(30));
    assert_eq!(j["tags"], serde_json::json!(["fast", "weak"]));

    // JSON → RON: structure survives (object → anonymous struct, array →
    // list). Re-parsing the produced RON yields a structurally-equal AST.
    let ron_text = reg.from_json(&id, &json_text).expect("from_json");
    let back = ast::parse(&ron_text).expect("re-parse produced RON");
    let to_json_again = ast::to_json(&back);
    assert_eq!(to_json_again["name"], serde_json::json!("goblin"));
    assert_eq!(to_json_again["hp"], serde_json::json!(30));
    assert_eq!(to_json_again["tags"], serde_json::json!(["fast", "weak"]));
}

// ── Tree-diff: variant/Option shape (struct/tuple name-match, synthetic
//    Some segment) ───────────────────────────────────────────────────────

#[test]
fn tree_diff_variant_and_option_shape() {
    // Changing a unit variant `Dark` → `Light` is one Modified leaf
    // (name-match is part of the shape), not a recurse.
    let src = "(\n  element: Dark,\n  fx: Some(5),\n)";
    let (mut reg, id) = open(src, Some("cfg.ron"));
    reg.set_text(&id, "(\n  element: Light,\n  fx: Some(9),\n)".to_string())
        .expect("set_text");

    let diff = reg.tree_diff(&id).expect("tree_diff");
    assert!(diff.change_count >= 2, "expected element + Some(inner) changes: {diff:?}");

    // The Option child diff goes through the synthetic "Some" segment.
    let fx = find_child(&diff, "fx").expect("fx node in diff");
    let some = find_child(fx, "Some").expect("synthetic Some segment");
    assert_eq!(some.path.last().map(String::as_str), Some("Some"));

    // Changing a named struct's variant name → Modified (not recurse).
    let src2 = "(\n  w: Sword(dmg: 1),\n)";
    let (mut reg2, id2) = open(src2, Some("cfg.ron"));
    reg2.set_text(&id2, "(\n  w: Axe(dmg: 1),\n)".to_string()).expect("set_text");
    let diff2 = reg2.tree_diff(&id2).expect("tree_diff");
    let w = find_child(&diff2, "w").expect("w node");
    assert_eq!(w.status, DiffStatus::Modified, "variant name change should be a Modified leaf");
}

/// Find a direct child of `node` by key.
fn find_child<'a>(node: &'a DiffTreeNode, key: &str) -> Option<&'a DiffTreeNode> {
    node.children.iter().find(|c| c.key == key)
}

// ── Encoding round-trip (windows-1252 / UTF-16 BOM) ──────────────────────

// windows-1252 round-trips byte-faithfully (encode→decode identity). UTF-16
// is decode-only in `encoding_rs`, so the UTF-16 half verifies the READ
// direction. Mirrors the json/toml/yaml crates' encoding test + the RON
// save path (`registry::write_to_disk` → `encode_for_disk_with_bom`).
#[test]
fn encoding_round_trip_windows1252_and_utf16() {
    use arbor_fs::prelude::encoding::{decode_bytes_full, encode_for_disk_with_bom};

    let original = "(\n  city: \"café\",\n)";
    let bytes = encode_for_disk_with_bom(original, Some("windows-1252"), false);
    assert!(bytes.contains(&0xE9), "windows-1252 encode missing 0xE9: {bytes:?}");
    let (decoded, _enc, had_bom) = decode_bytes_full(&bytes);
    assert!(!had_bom);
    assert_eq!(decoded, original, "windows-1252 round-trip must be identity");

    let utf16_src = "(\n  k: \"välue\",\n)";
    let mut u16_bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in utf16_src.encode_utf16() {
        u16_bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let (decoded16, _e, had_bom16) = decode_bytes_full(&u16_bytes);
    assert!(had_bom16, "UTF-16LE BOM not detected on decode");
    assert_eq!(
        decoded16.trim_start_matches('\u{feff}'),
        utf16_src,
        "UTF-16LE decode must recover the text",
    );
}
