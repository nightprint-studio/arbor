//! §6 .properties round-trip tests — the crown-jewel preservation
//! checks, exercising the `SimpleFormat` seam directly (parse / mutate /
//! project), which is what `DefaultBackend` drives.

use super::*;
use arbor_studio_core::prelude::{History, SimpleMutation};
use serde_json::Value;

fn fmt() -> PropertiesFormat {
    PropertiesFormat::new()
}

fn enc() -> EncodingInfo {
    EncodingInfo::utf8()
}

/// Continuation lines (trailing `\`) survive a parse → emit round-trip
/// byte-for-byte, and the joined logical value is decoded.
#[test]
fn continuation_lines_survive() {
    let src = "long.value=abc\\\n  def\nother=x\n";
    // Projected value sees the joined string.
    let v = fmt().parse(src, &enc()).value.unwrap();
    assert_eq!(v.pointer("/long/value"), Some(&Value::String("abcdef".into())));
    // An untouched mutation path (set `other`) keeps the continuation
    // line intact in the emitted text.
    let out = fmt()
        .mutate(
            src,
            SimpleMutation::SetPrimitive {
                path:  vec!["other".into()],
                value: Value::String("y".into()),
            },
        )
        .unwrap();
    assert!(out.contains("long.value=abc\\\n  def\n"), "continuation preserved: {out:?}");
    assert!(out.contains("other=y\n"));
}

/// Key/value escapes (`\=`, `\:`, `\n`, `\uXXXX`) — escaped separators
/// in the key are part of the key, `\n` decodes in the value, `\uXXXX`
/// decodes in the KEY (the value path runs `unescape_value` before
/// `decode_unicode`, a pre-extraction quirk preserved here), and the raw
/// source round-trips byte-identical when untouched (lossless emit).
#[test]
fn escapes_round_trip() {
    // `a\=b` is a single key "a=b"; value has a `\n` escape.
    let src = "a\\=b=line1\\nline2\n";
    let parse = fmt().parse(src, &enc());
    let v = parse.value.unwrap();
    assert_eq!(
        v.get("a=b"),
        Some(&Value::String("line1\nline2".into())),
        "escaped `=` in key + `\\n` in value decoded: {v:?}",
    );

    // Colon separator + escaped colon in key.
    let src2 = "url\\:host:localhost\n";
    let v2 = fmt().parse(src2, &enc()).value.unwrap();
    assert_eq!(v2.get("url:host"), Some(&Value::String("localhost".into())));

    // `\uXXXX` decodes in the KEY (the key path runs decode_unicode after
    // a non-consuming unescape).
    let src3 = "caf\\u00e9.port=8080\n";
    let v3 = fmt().parse(src3, &enc()).value.unwrap();
    assert_eq!(v3.pointer("/café/port"), Some(&Value::String("8080".into())));

    // Lossless emit: an untouched mutation of a sibling preserves the
    // escape bytes byte-for-byte.
    let out = fmt()
        .mutate(
            "a\\=b=v\\u00e9w\nother=z\n",
            SimpleMutation::SetPrimitive {
                path:  vec!["other".into()],
                value: Value::String("z2".into()),
            },
        )
        .unwrap();
    assert!(out.contains("a\\=b=v\\u00e9w\n"), "escape bytes preserved: {out:?}");
}

/// `$value` sentinel for a prefix that is BOTH a leaf and a container
/// (`foo=v` + `foo.bar=w`).
#[test]
fn value_sentinel_for_prefix_collision() {
    let src = "foo=bar\nfoo.sub=baz\n";
    let parse = fmt().parse(src, &enc());
    assert!(parse.error.is_none(), "collisions are legal — no warning");
    let v = parse.value.unwrap();
    assert_eq!(v.pointer("/foo/$value"), Some(&Value::String("bar".into())));
    assert_eq!(v.pointer("/foo/sub"),    Some(&Value::String("baz".into())));
}

/// Every key is projected (the every-key-is-ref projection): a flat
/// dotted key shows up as nested objects; bracket indices become arrays.
#[test]
fn every_key_projected() {
    let src = "server.port=8080\nservers[0]=alpha\nservers[1]=beta\n";
    let v = fmt().parse(src, &enc()).value.unwrap();
    assert_eq!(v.pointer("/server/port"), Some(&Value::String("8080".into())));
    let arr = v.pointer("/servers").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], Value::String("alpha".into()));
    assert_eq!(arr[1], Value::String("beta".into()));
}

/// Undo after a tree mutation restores the byte-identical original.
/// Mirrors `DefaultBackend`'s history pipeline (dedup ON for .properties,
/// but a real structural change still records a step).
#[test]
fn undo_after_tree_mutation_byte_identical() {
    let src = "# keep me\nserver.port=8080\nserver.host=localhost\n";
    let mut hist = History::new_dedup(src.to_string(), 200);
    let mutated = fmt()
        .mutate(
            src,
            SimpleMutation::SetPrimitive {
                path:  vec!["server".into(), "port".into()],
                value: Value::String("9090".into()),
            },
        )
        .unwrap();
    hist.record_struct(mutated.clone());
    assert_ne!(hist.current(), src);
    let undone = hist.undo().unwrap();
    assert_eq!(undone, src, "undo must restore byte-identical original");
    // The comment + separator survived the set.
    assert!(mutated.contains("# keep me\n"));
    assert!(mutated.contains("server.port=9090\n"));
    assert!(mutated.contains("server.host=localhost\n"));
}

/// dedup history: a replayed identical snapshot is a no-op (no extra undo
/// step). This is the .properties-specific `dedup=true` behavior.
#[test]
fn dedup_suppresses_noop_snapshot() {
    let src = "a=1\n";
    let mut hist = History::new_dedup(src.to_string(), 200);
    // Replaying the same text must not create an undoable step.
    hist.record_struct(src.to_string());
    assert!(!hist.can_undo(), "identical snapshot suppressed under dedup");
}

/// Setting an existing key preserves its separator + surrounding
/// whitespace.
#[test]
fn set_preserves_separator() {
    let src = "server.port = 8080\nserver.host=localhost\n";
    let out = fmt()
        .mutate(
            src,
            SimpleMutation::SetPrimitive {
                path:  vec!["server".into(), "port".into()],
                value: Value::String("9090".into()),
            },
        )
        .unwrap();
    assert!(out.contains("server.port = 9090\n"), "separator kept: {out:?}");
    assert!(out.contains("server.host=localhost\n"));
}

/// Removing a container key drops every descendant.
#[test]
fn remove_container_drops_subkeys() {
    let src = "server.port=8080\nserver.host=localhost\nother=v\n";
    let out = fmt()
        .mutate(src, SimpleMutation::RemoveAt { path: vec!["server".into()] })
        .unwrap();
    assert!(!out.contains("server."), "subkeys gone: {out:?}");
    assert!(out.contains("other=v\n"));
}

/// Encoding round-trip identity through the same encode/decode the core
/// persist layer uses (FROZEN F16). windows-1252 full identity; UTF-16
/// decode-direction (encoding_rs has no UTF-16 encoder).
#[test]
fn encoding_round_trip() {
    let text = "name=café\n";

    let bytes = arbor_fs::prelude::encoding::encode_for_disk_with_bom(
        text,
        Some("windows-1252"),
        false,
    );
    let (decoded, _enc, _bom) = arbor_fs::prelude::encoding::decode_bytes_full(&bytes);
    assert_eq!(decoded, text, "windows-1252 round-trip must be identity");

    let mut u16_bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        u16_bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let (decoded16, _e, had_bom16) =
        arbor_fs::prelude::encoding::decode_bytes_full(&u16_bytes);
    assert_eq!(
        decoded16.trim_start_matches('\u{feff}'),
        text,
        "UTF-16LE decode must recover the text",
    );
    assert!(had_bom16, "UTF-16LE BOM must be detected");
}
