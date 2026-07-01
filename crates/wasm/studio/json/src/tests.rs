//! §6 JSON crown-jewel tests — round-trip preservation + the json-special
//! behaviors. Pure logic against the registry / AST; no Tauri, no FS
//! destructive ops (the encoding test round-trips through `arbor-fs` in
//! memory, mirroring the save path).

use serde_json::json;

use crate::registry::{DocParseMode, JsonStudioRegistry};

const BIG_THRESHOLD: usize = 1024 * 1024;

/// Open a doc in tree mode (small file) and return (registry, doc_id).
fn open_tree(text: &str, path: Option<&str>) -> (JsonStudioRegistry, String) {
    let mut reg = JsonStudioRegistry::default();
    let res = reg.parse(
        text.to_string(),
        path.map(|p| p.to_string()),
        "utf-8".to_string(),
        false,
        BIG_THRESHOLD,
    );
    let id = res.doc_id;
    (reg, id)
}

// ── JSONC comments + trailing commas survive a scalar edit ───────────────

#[test]
fn jsonc_comments_and_trailing_commas_survive_scalar_edit() {
    let src = "{\n  // leading comment\n  \"a\": 1,\n  \"b\": 2,\n}";
    let (mut reg, id) = open_tree(src, Some("conf.jsonc"));

    // Editing `a` from 1 → 42 is a byte-splice over just the value span;
    // the comment + trailing comma sit outside every touched span.
    let res = reg
        .mutate_primitive(&id, &["a".to_string()], json!(42))
        .expect("mutate scalar");

    assert!(res.text.contains("// leading comment"), "comment dropped: {}", res.text);
    assert!(res.text.contains("\"b\": 2,\n}"), "trailing comma dropped: {}", res.text);
    assert!(res.text.contains("\"a\": 42"), "edit not applied: {}", res.text);
    assert!(res.has_jsonc_features, "jsonc features flag lost after edit");
}

// ── Stream-mode threshold selects the right parse path ───────────────────

#[test]
fn stream_threshold_selects_parse_mode() {
    // Below threshold → Tree mode (AST + full editing).
    let small = "{\"a\": 1}";
    let mut reg = JsonStudioRegistry::default();
    let r1 = reg.parse(small.to_string(), None, "utf-8".into(), false, 16);
    assert_eq!(r1.parse_mode, DocParseMode::Tree);

    // At/above threshold → Stream mode (simd_json, navigation-only). The
    // 8-byte doc is `>= 8`.
    let r2 = reg.parse(small.to_string(), None, "utf-8".into(), false, small.len());
    assert_eq!(r2.parse_mode, DocParseMode::Stream);

    // Structural mutation is disabled in stream mode.
    let err = reg.mutate_primitive(&r2.doc_id, &["a".into()], json!(2));
    assert!(err.is_err(), "stream-mode mutation should be Unsupported");
}

// ── strip_features removes comments only when invoked ────────────────────

#[test]
fn strip_features_removes_comments_only_when_invoked() {
    let src = "{\n  // a note\n  \"a\": 1, // trailing\n}";
    let (mut reg, id) = open_tree(src, Some("conf.jsonc"));

    // Before strip: comments still present in the live buffer.
    assert!(reg.raw_current(&id).unwrap().contains("// a note"));

    let res = reg.strip_jsonc_features(&id).expect("strip");
    assert!(!res.text.contains("// a note"), "strip kept a comment: {}", res.text);
    assert!(!res.text.contains("// trailing"), "strip kept a comment: {}", res.text);
    assert!(!res.has_jsonc_features, "features flag should clear after strip");
    // The data survives the reformat.
    let v: serde_json::Value = serde_json::from_str(&res.text).expect("valid json after strip");
    assert_eq!(v, json!({ "a": 1 }));
}

// ── Undo after a mutation restores the byte-identical original ────────────

#[test]
fn undo_after_mutation_is_byte_identical() {
    let src = "{\n  \"a\": 1,\n  \"b\": \"x\"\n}";
    let (mut reg, id) = open_tree(src, Some("conf.json"));

    reg.mutate_primitive(&id, &["a".to_string()], json!(99)).expect("mutate");
    assert_ne!(reg.raw_current(&id).unwrap(), src);

    let undone = reg.undo(&id).expect("undo");
    assert_eq!(undone.text, src, "undo not byte-identical");
    assert_eq!(reg.raw_current(&id).unwrap(), src);
}

// ── Encoding round-trip (windows-1252 / UTF-16 BOM) ──────────────────────

// windows-1252 round-trips byte-faithfully (encode→decode identity). UTF-16
// is decode-only in `encoding_rs` (no UTF-16 encoder in the WHATWG set), so
// the UTF-16 half verifies the READ direction — the part that matters for
// preserving content the user opened in a UTF-16 file. Mirrors the
// toml/yaml crates' encoding test.
#[test]
fn encoding_round_trip_windows1252_and_utf16() {
    use arbor_fs::prelude::encoding::{decode_bytes_full, encode_for_disk_with_bom};

    // "café" — the é is 0xE9 in windows-1252 (distinct from UTF-8 0xC3 0xA9).
    let original = "{\"city\": \"café\"}";
    let bytes = encode_for_disk_with_bom(original, Some("windows-1252"), false);
    assert!(bytes.contains(&0xE9), "windows-1252 encode missing 0xE9: {bytes:?}");
    let (decoded, _enc, had_bom) = decode_bytes_full(&bytes);
    assert!(!had_bom);
    assert_eq!(decoded, original, "windows-1252 round-trip must be identity");

    // UTF-16LE decode direction: BOM (0xFF 0xFE) + LE code units.
    let utf16_src = "{\"k\": \"välue\"}";
    let mut u16_bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in utf16_src.encode_utf16() {
        u16_bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let (decoded16, _e, had_bom16) = decode_bytes_full(&u16_bytes);
    assert!(had_bom16, "UTF-16LE BOM not detected on decode");
    // `decode_bytes_full` keeps the BOM char in the string (callers
    // re-prepend on write via `had_bom`); strip it before comparing content.
    assert_eq!(
        decoded16.trim_start_matches('\u{feff}'),
        utf16_src,
        "UTF-16LE decode must recover the text",
    );
}

// ── Tree-diff distinguishes 1.0 vs 1.00 (Number.raw — json-special) ──────

#[test]
fn tree_diff_distinguishes_number_raw() {
    let src = "{\n  \"x\": 1.0\n}";
    let (mut reg, id) = open_tree(src, Some("conf.json"));

    // Same numeric value, different literal text. A value-based diff would
    // call these equal; JSON's AST diff compares `Number.raw` → Modified.
    reg.set_text(&id, "{\n  \"x\": 1.00\n}".to_string()).expect("set_text");

    let diff = reg.tree_diff(&id).expect("tree_diff");
    assert!(diff.change_count >= 1, "1.0 vs 1.00 reported as unchanged (raw not compared)");

    // Sanity: a no-op set_text of the identical literal is unchanged.
    let (mut reg2, id2) = open_tree(src, Some("conf.json"));
    reg2.set_text(&id2, src.to_string()).expect("set_text noop");
    let diff2 = reg2.tree_diff(&id2).expect("tree_diff noop");
    assert_eq!(diff2.change_count, 0, "identical text should diff clean");
}
