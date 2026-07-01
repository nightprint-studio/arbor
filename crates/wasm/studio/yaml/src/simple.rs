//! [`YamlFormat`] — the [`SimpleFormat`] impl that lets YAML ride on
//! `arbor_studio_core::DefaultBackend`.
//!
//! Format-specific work is delegated to the sibling modules: `project`
//! (parse + JSON projection + multi-doc split + indent + emit), `mutate`
//! (the structured mutation lowering), `kind` (node-kind / preview),
//! `descriptor` (the capability matrix). The backend owns all the
//! boilerplate.

use arbor_studio_core::prelude::{
    EncodingInfo, FormatDescriptor, ParseOutcome, SimpleFormat, SimpleMutation, StudioError,
    StudioResult,
};
use serde_json::Value;

use crate::{descriptor, kind, mutate, project};

/// The YAML format primitives for [`arbor_studio_core::prelude::DefaultBackend`].
pub struct YamlFormat {
    descriptor: FormatDescriptor,
}

impl YamlFormat {
    pub fn new() -> Self {
        Self { descriptor: descriptor::build_descriptor() }
    }
}

impl Default for YamlFormat {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleFormat for YamlFormat {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn parse(&self, text: &str, _encoding: &EncodingInfo) -> ParseOutcome {
        let outcome = project::parse_outcome(text);
        ParseOutcome { value: outcome.value, error: outcome.error }
    }

    fn detect_indent(&self, text: &str) -> String {
        project::detect_indent(text)
    }

    fn pretty(&self, text: &str) -> StudioResult<String> {
        // `yaml_edit` preserves formatting natively; re-emit each parsed
        // `Document` and re-join with the canonical `---` separator. The
        // round-trip normalises only what the user's text couldn't already
        // represent (stray trailing whitespace, mixed-style indent).
        let (parsed, _value, parse_error, _count, multi) = project::parse_text(text);
        if let Some(e) = parse_error {
            return Err(StudioError::App(format!(
                "Document has parse errors — cannot pretty-print: {e}"
            )));
        }
        let docs = parsed.ok_or_else(|| {
            StudioError::App("Document has parse errors — cannot pretty-print".into())
        })?;
        Ok(project::join_documents(&docs, multi))
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

    // `variant_tag` defaults to None (YAML has no variant tags).
}

#[cfg(test)]
mod tests {
    //! §6 YAML round-trip tests — the crown-jewel preservation checks,
    //! exercising the `SimpleFormat` seam directly (parse / mutate /
    //! project), which is what `DefaultBackend` drives.

    use super::*;
    use arbor_studio_core::prelude::{History, SimpleMutation};

    fn fmt() -> YamlFormat {
        YamlFormat::new()
    }

    fn enc() -> EncodingInfo {
        EncodingInfo::utf8()
    }

    /// Scalar `SetPrimitive` rides `yaml_edit::set_path`: the targeted
    /// line's own trailing inline comment survives and the untouched
    /// subtree (comments + key order) is left byte-for-byte alone — only
    /// the value changes.
    ///
    /// NOTE (behavior-preserving): `yaml-edit` 0.2's `set_path` drops a
    /// *leading* comment / blank line attached to the edited key's line
    /// when that line also carries a trailing inline comment. This is the
    /// exact pre-extraction launcher behavior (same `set_path` call), so we
    /// assert only what survives, and explicitly cover the untouched
    /// subtree below.
    #[test]
    fn scalar_set_lossless_keeps_comments() {
        let src = "\
name: old   # trailing comment

# server config
server:
  port: 8080   # keep me
  host: localhost
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
        assert!(out.contains("# trailing comment"), "trailing comment survived: {out}");
        assert!(out.contains("new"), "value changed: {out}");
        assert!(!out.contains("old"), "old value gone: {out}");
        // Untouched subtree intact: its comments survive + key order kept.
        assert!(out.contains("# server config"), "subtree comment survived: {out}");
        assert!(out.contains("# keep me"), "inline subtree comment survived: {out}");
        let server = out.split("server:").nth(1).unwrap();
        let port_at = server.find("port").unwrap();
        let host_at = server.find("host").unwrap();
        assert!(port_at < host_at);
    }

    /// A scalar set re-projects to the new value (drives the tree pane).
    #[test]
    fn scalar_set_reprojects() {
        let src = "title: old\ncount: 1\n";
        let out = fmt()
            .mutate(
                src,
                SimpleMutation::SetPrimitive {
                    path:  vec!["title".into()],
                    value: Value::String("new".into()),
                },
            )
            .unwrap();
        let v = fmt().parse(&out, &enc()).value.unwrap();
        assert_eq!(v["title"], Value::String("new".into()));
        assert_eq!(v["count"], Value::Number(1.into()));
    }

    /// Multi-doc `---` streams round-trip: the projection is an Array of
    /// per-doc objects, and re-emit preserves the separator + both docs.
    #[test]
    fn multi_doc_split_round_trips() {
        let src = "\
name: first
value: 1
---
name: second
value: 2
";
        let v = fmt().parse(src, &enc()).value.unwrap();
        // Multi-doc projects to an Array of objects.
        let arr = v.as_array().expect("multi-doc projects to array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], Value::String("first".into()));
        assert_eq!(arr[1]["name"], Value::String("second".into()));

        // A set into the second doc (index-prefixed path) keeps both docs
        // and the separator.
        let out = fmt()
            .mutate(
                src,
                SimpleMutation::SetPrimitive {
                    path:  vec!["1".into(), "name".into()],
                    value: Value::String("changed".into()),
                },
            )
            .unwrap();
        assert!(out.contains("---"), "separator survived: {out}");
        assert!(out.contains("name: first"));
        assert!(out.contains("changed"));
        let v2 = fmt().parse(&out, &enc()).value.unwrap();
        assert_eq!(v2[0]["name"], Value::String("first".into()));
        assert_eq!(v2[1]["name"], Value::String("changed".into()));
    }

    /// YAML `null` is first-class (descriptor null_handling = Native): the
    /// projection carries a real `Value::Null`, and a `null` set writes a
    /// literal null rather than deleting the key.
    #[test]
    fn null_is_native() {
        let src = "a: ~\nb: 1\n";
        let v = fmt().parse(src, &enc()).value.unwrap();
        assert_eq!(v["a"], Value::Null);
        assert_eq!(crate::kind::node_kind(&v["a"]), "null");

        let out = fmt()
            .mutate(
                src,
                SimpleMutation::SetPrimitive {
                    path:  vec!["b".into()],
                    value: Value::Null,
                },
            )
            .unwrap();
        let v2 = fmt().parse(&out, &enc()).value.unwrap();
        assert_eq!(v2["b"], Value::Null, "b set to literal null: {out}");
        // Key still present (Native null, not a delete).
        assert!(v2.as_object().unwrap().contains_key("b"));
    }

    /// Undo after a tree mutation restores byte-identical original. Mirrors
    /// the `DefaultBackend` history pipeline.
    #[test]
    fn undo_after_tree_mutation_byte_identical() {
        let src = "\
# keep me
name: old
count: 1
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

    /// RemoveAt deletes the key.
    #[test]
    fn remove_deletes_key() {
        let src = "a: 1\nb: 2\n";
        let out = fmt()
            .mutate(src, SimpleMutation::RemoveAt { path: vec!["a".into()] })
            .unwrap();
        let v = fmt().parse(&out, &enc()).value.unwrap();
        assert!(!v.as_object().unwrap().contains_key("a"), "a removed: {out}");
        assert_eq!(v["b"], Value::Number(2.into()));
    }

    /// Encoding round-trip identity through the same encode/decode the core
    /// persist layer uses (FROZEN F16). windows-1252 full identity; UTF-16
    /// decode-direction (encoding_rs has no UTF-16 encoder).
    #[test]
    fn encoding_round_trip() {
        let text = "name: café\n";

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
}
