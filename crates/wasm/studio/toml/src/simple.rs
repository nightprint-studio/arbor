//! [`TomlFormat`] — the [`SimpleFormat`] impl that lets TOML ride on
//! `arbor_studio_core::DefaultBackend`.
//!
//! Everything format-specific is delegated to the sibling modules:
//! `project` (parse + JSON projection + indent), `mutate` (the structured
//! mutation lowering), `kind` (node-kind / preview), `descriptor` (the
//! capability matrix). The backend owns all the boilerplate.

use arbor_studio_core::prelude::{
    EncodingInfo, FormatDescriptor, ParseOutcome, SimpleFormat, SimpleMutation, StudioError,
    StudioResult,
};
use serde_json::Value;

use crate::{descriptor, kind, mutate, project};

/// The TOML format primitives for [`arbor_studio_core::prelude::DefaultBackend`].
pub struct TomlFormat {
    descriptor: FormatDescriptor,
}

impl TomlFormat {
    pub fn new() -> Self {
        Self { descriptor: descriptor::build_descriptor() }
    }
}

impl Default for TomlFormat {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleFormat for TomlFormat {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn parse(&self, text: &str, _encoding: &EncodingInfo) -> ParseOutcome {
        let (_doc, value, error) = project::parse_pair(text);
        ParseOutcome { value, error }
    }

    fn detect_indent(&self, text: &str) -> String {
        project::detect_indent(text)
    }

    fn pretty(&self, text: &str) -> StudioResult<String> {
        // `toml_edit` preserves formatting natively; route through a fresh
        // `DocumentMut::to_string()` so any decor anomalies (missing
        // trailing newline) normalise.
        let doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
            StudioError::App(format!("Document has parse errors — cannot pretty-print: {e}"))
        })?;
        Ok(doc.to_string())
    }

    fn mutate(&self, text: &str, mutation: SimpleMutation) -> StudioResult<String> {
        mutate::mutate(text, mutation)
    }

    fn node_kind(&self, v: &Value) -> String {
        kind::node_kind(v)
    }

    fn preview_for(&self, v: &Value) -> String {
        kind::preview_for(v)
    }

    // `variant_tag` defaults to None (TOML has no variant tags).
}

#[cfg(test)]
mod tests {
    //! §6 TOML round-trip tests — the crown-jewel preservation checks.
    //! These exercise the `SimpleFormat` seam directly (parse / mutate /
    //! project), which is exactly what `DefaultBackend` drives.

    use super::*;
    use arbor_studio_core::prelude::{History, SimpleMutation};

    fn fmt() -> TomlFormat {
        TomlFormat::new()
    }

    fn enc() -> EncodingInfo {
        EncodingInfo::utf8()
    }

    /// Comments / whitespace / key-ordering survive a scalar edit (toml_edit
    /// decor preservation).
    #[test]
    fn decor_survives_scalar_edit() {
        let src = "\
# leading comment
name = \"old\"   # trailing comment

[server]
port = 8080
host = \"localhost\"
";
        let out = fmt()
            .mutate(
                src,
                SimpleMutation::SetPrimitive {
                    path:  vec!["name".into()],
                    value: Value::String("new".into()),
                },
            )
            .unwrap();
        // Comments + the trailing inline comment + blank line + key order
        // all preserved; only the value changed.
        assert!(out.contains("# leading comment"));
        assert!(out.contains("# trailing comment"));
        assert!(out.contains("name = \"new\""));
        assert!(!out.contains("\"old\""));
        // Key order under [server] unchanged.
        let server = out.split("[server]").nth(1).unwrap();
        let port_at = server.find("port").unwrap();
        let host_at = server.find("host").unwrap();
        assert!(port_at < host_at);
    }

    /// Array-of-tables vs array vs inline-table are distinct kinds in the
    /// LIVE projection navigation — array-of-tables projects to an array of
    /// objects, a value array projects to an array, an inline table to an
    /// object. (FROZEN F11: kind strings stay distinct.)
    #[test]
    fn container_kinds_distinct() {
        let src = "\
arr = [1, 2, 3]
inline = { a = 1, b = 2 }

[[products]]
id = \"a\"

[[products]]
id = \"b\"
";
        let v = fmt().parse(src, &enc()).value.unwrap();
        // value array → array
        assert_eq!(kind::node_kind(&v["arr"]), "array");
        // inline table → inline_table (object projection)
        assert_eq!(kind::node_kind(&v["inline"]), "inline_table");
        // array-of-tables projects to an array of objects
        assert_eq!(kind::node_kind(&v["products"]), "array");
        assert_eq!(kind::node_kind(&v["products"][0]), "inline_table");
        // and the two AoT entries are distinct objects, in order
        assert_eq!(v["products"][0]["id"], Value::String("a".into()));
        assert_eq!(v["products"][1]["id"], Value::String("b".into()));
    }

    /// A datetime literal is preserved through an unrelated edit.
    #[test]
    fn datetime_preserved() {
        let src = "\
created = 1979-05-27T07:32:00Z
title = \"old\"
";
        let out = fmt()
            .mutate(
                src,
                SimpleMutation::SetPrimitive {
                    path:  vec!["title".into()],
                    value: Value::String("new".into()),
                },
            )
            .unwrap();
        assert!(out.contains("created = 1979-05-27T07:32:00Z"));
        assert!(out.contains("title = \"new\""));
    }

    /// null on a scalar `set` is rejected at the value-conversion layer
    /// (TOML has no null — the descriptor's AsDelete policy routes deletes
    /// through RemoveAt instead).
    #[test]
    fn null_set_rejected() {
        let src = "title = \"x\"\n";
        let res = fmt().mutate(
            src,
            SimpleMutation::SetPrimitive {
                path:  vec!["title".into()],
                value: Value::Null,
            },
        );
        assert!(res.is_err());
    }

    /// RemoveAt deletes the key (the AsDelete flow's actual mutation).
    #[test]
    fn remove_deletes_key() {
        let src = "a = 1\nb = 2\n";
        let out = fmt()
            .mutate(src, SimpleMutation::RemoveAt { path: vec!["a".into()] })
            .unwrap();
        assert!(!out.contains("a = 1"));
        assert!(out.contains("b = 2"));
    }

    /// Undo after a tree mutation restores byte-identical original. This
    /// simulates the `DefaultBackend` history pipeline: snapshot original,
    /// record a structural edit, undo → back to the exact original text.
    #[test]
    fn undo_after_tree_mutation_byte_identical() {
        let src = "\
# keep me
name = \"old\"
count = 1
";
        let mut hist = History::new(src.to_string(), 200);
        let mutated = fmt()
            .mutate(
                src,
                SimpleMutation::SetPrimitive {
                    path:  vec!["name".into()],
                    value: Value::String("new".into()),
                },
            )
            .unwrap();
        hist.record_struct(mutated.clone());
        assert_ne!(hist.current(), src);
        let undone = hist.undo().unwrap();
        assert_eq!(undone, src, "undo must restore byte-identical original");
    }

    /// Encoding round-trip identity through the same encode/decode the core
    /// persist layer uses. Guards FROZEN F16: a multi-file refactor must
    /// preserve each file's content across the decode → re-encode flush.
    ///
    /// windows-1252 (é = 0xE9, distinct from UTF-8's 0xC3 0xA9) round-trips
    /// byte-faithfully. UTF-16 is a decode-only encoding in `encoding_rs`
    /// (the WHATWG encoder set has no UTF-16 target), so we verify the READ
    /// direction: a BOM-bearing UTF-16LE byte stream decodes to the exact
    /// text — which is the half that matters for preserving content the
    /// user opened in a UTF-16 file.
    #[test]
    fn encoding_round_trip() {
        let text = "name = \"café\"\n";

        // windows-1252 — full encode→decode identity.
        let bytes = arbor_fs::prelude::encoding::encode_for_disk_with_bom(
            text,
            Some("windows-1252"),
            false,
        );
        let (decoded, _enc, _bom) = arbor_fs::prelude::encoding::decode_bytes_full(&bytes);
        assert_eq!(decoded, text, "windows-1252 round-trip must be identity");

        // UTF-16LE — decode direction: BOM (0xFF 0xFE) + LE code units.
        let mut u16_bytes: Vec<u8> = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            u16_bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let (decoded16, _e, had_bom16) =
            arbor_fs::prelude::encoding::decode_bytes_full(&u16_bytes);
        // `decode_without_bom_handling` keeps the BOM char in the string
        // (callers re-prepend on write via `had_bom`), so strip it before
        // comparing the content.
        assert_eq!(
            decoded16.trim_start_matches('\u{feff}'),
            text,
            "UTF-16LE decode must recover the text",
        );
        assert!(had_bom16, "UTF-16LE BOM must be detected");
    }
}
