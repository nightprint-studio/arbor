//! Validates `DefaultBackend<F>` against a trivial in-test `DummyFormat`,
//! proving the generic is sound (it satisfies `StudioFormatBackend` and
//! round-trips parse → mutate → project → emit + undo) BEFORE the real
//! TOML/YAML/.properties crates adopt it (their crown-jewel round-trip
//! tests ship with each crate in Stage 3).

use serde_json::{json, Value};

use arbor_studio_types::prelude::{
    EncodingInfo, FormatDescriptor, IconRef, NullPolicy, QuerySyntax, StudioMutation, StudioResult,
};

use crate::default_backend::{DefaultBackend, SchemaRouting};
use crate::simple::{ParseOutcome, SimpleFormat, SimpleMutation};
use crate::backend::StudioFormatBackend;

/// A toy line-oriented format: `key=value` per line, flat string map.
/// Just enough structure to drive every `SimpleFormat` primitive the
/// backend calls, without pulling a real parser into `core`'s deps.
struct DummyFormat {
    descriptor: FormatDescriptor,
}

impl DummyFormat {
    fn new() -> Self {
        Self { descriptor: dummy_descriptor() }
    }

    /// Parse `key=value\n` lines into an ordered string map.
    fn to_map(text: &str) -> Vec<(String, String)> {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn emit(map: &[(String, String)]) -> String {
        let mut out = String::new();
        for (k, v) in map {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
        out
    }
}

impl SimpleFormat for DummyFormat {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn parse(&self, text: &str, _encoding: &EncodingInfo) -> ParseOutcome {
        let mut obj = serde_json::Map::new();
        for (k, v) in Self::to_map(text) {
            obj.insert(k, Value::String(v));
        }
        ParseOutcome::ok(Value::Object(obj))
    }

    fn detect_indent(&self, _text: &str) -> String {
        "  ".into()
    }

    fn pretty(&self, text: &str) -> StudioResult<String> {
        Ok(Self::emit(&Self::to_map(text)))
    }

    fn mutate(&self, text: &str, mutation: SimpleMutation) -> StudioResult<String> {
        let mut map = Self::to_map(text);
        match mutation {
            SimpleMutation::SetPrimitive { path, value } => {
                let key = path.last().cloned().unwrap_or_default();
                let val = match value {
                    Value::String(s) => s,
                    other            => other.to_string(),
                };
                if let Some(e) = map.iter_mut().find(|(k, _)| *k == key) {
                    e.1 = val;
                } else {
                    map.push((key, val));
                }
            }
            SimpleMutation::RemoveAt { path } => {
                let key = path.last().cloned().unwrap_or_default();
                map.retain(|(k, _)| *k != key);
            }
            SimpleMutation::InsertField { name, text: snippet, .. } => {
                map.push((name, snippet));
            }
            // The other variants aren't exercised by these tests; treat
            // them as no-ops so the dummy stays minimal.
            _ => {}
        }
        Ok(Self::emit(&map))
    }

    fn node_kind(&self, v: &Value) -> String {
        match v {
            Value::Object(_) => "object",
            Value::Array(_)  => "array",
            Value::Null      => "null",
            _                => "string",
        }
        .to_string()
    }

    fn preview_for(&self, v: &Value) -> String {
        match v {
            Value::String(s) => format!("\"{s}\""),
            Value::Object(m) => format!("{{{} keys}}", m.len()),
            other            => other.to_string(),
        }
    }
}

fn dummy_descriptor() -> FormatDescriptor {
    FormatDescriptor {
        id:                        "dummy".into(),
        label:                     "Dummy".into(),
        file_extensions:           vec![".dummy".into()],
        icon:                      IconRef::Iconify { name: "x".into() },
        supports_lossless_edit:    true,
        supports_comments:         false,
        supports_anchors:          false,
        null_handling:             NullPolicy::Native,
        supports_streaming_mode:   false,
        streaming_threshold_kb:    None,
        streaming_setting_key:     None,
        query_syntax:              QuerySyntax::JsonPath,
        cross_ref_default_fields:  vec![],
        cross_ref_scopes:          vec![],
        schema_sources:            vec![],
        kind_palette:              Default::default(),
        save_warnings:             vec![],
        save_behavior_setting_key: None,
        convert_to_json_supported: false,
        supports_external_files:   false,
        supports_rename_reference: false,
        supports_bulk_edit:        false,
    }
}

fn backend() -> DefaultBackend<DummyFormat> {
    DefaultBackend::new(DummyFormat::new(), SchemaRouting::None, /* dedup */ false)
}

/// Minimal single-poll executor. The async trait methods exercised here
/// (`parse`, `get_encoding`) never actually `.await` a pending future —
/// the only async work behind them is synchronous registry access — so a
/// poll-once driver resolves them without pulling a tokio dev-dependency
/// into `core` (CLAUDE.md: no new libraries without asking).
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn noop_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker { noop_raw_waker() }
        let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
        RawWaker::new(std::ptr::null(), vtable)
    }

    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending  => panic!("future pended — test fixture futures must resolve synchronously"),
    }
}

/// `DefaultBackend<DummyFormat>` is usable as a `dyn StudioFormatBackend`
/// — i.e. the generic satisfies the trait object the registry stores.
#[test]
fn satisfies_studio_format_backend() {
    let b = backend();
    let _obj: &dyn StudioFormatBackend = &b;
    assert_eq!(b.descriptor().id, "dummy");
}

/// parse → project → mutate → project → emit, then undo restores the
/// original buffer byte-for-byte.
#[test]
fn parse_mutate_project_emit_undo_roundtrip() {
    let b = backend();
    let original = "a=1\nb=2\n".to_string();
    let parsed = block_on(b
        .parse(original.clone(), Some("/x.dummy".into()), EncodingInfo::utf8()))
        .unwrap();
    let id = parsed.doc_id;

    // Projection (root + children).
    assert_eq!(parsed.root_kind.as_deref(), Some("object"));
    assert_eq!(parsed.child_count, 2);
    let children = b.get_children(&id, vec![]).unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].key, "a");
    assert_eq!(children[0].kind, "string");

    // Mutate: set `a` to "9".
    let res = b
        .apply_mutation(&id, StudioMutation::SetPrimitive {
            path:  vec!["a".into()],
            value: json!("9"),
        })
        .unwrap();
    assert_eq!(res.text, "a=9\nb=2\n");
    assert!(res.can_undo);
    // Projection reflects the edit.
    assert_eq!(b.get_value(&id, vec!["a".into()]).unwrap(), "\"9\"");

    // Undo restores the original buffer exactly.
    let undone = b.undo(&id).unwrap();
    assert_eq!(undone.text, original);
    assert_eq!(b.raw_current(&id).unwrap(), original);

    // Redo re-applies.
    let redone = b.redo(&id).unwrap();
    assert_eq!(redone.text, "a=9\nb=2\n");
}

/// Structural mutations record discrete undo steps (no coalescing across
/// them) — proving the backend wires `record_struct` for tree edits.
#[test]
fn structural_edits_are_discrete_undo_steps() {
    let b = backend();
    let parsed = block_on(b
        .parse("a=1\n".to_string(), None, EncodingInfo::utf8()))
        .unwrap();
    let id = parsed.doc_id;

    b.apply_mutation(&id, StudioMutation::InsertField {
        path: vec![],
        name: "b".into(),
        text: "2".into(),
    })
    .unwrap();
    b.apply_mutation(&id, StudioMutation::InsertField {
        path: vec![],
        name: "c".into(),
        text: "3".into(),
    })
    .unwrap();

    assert_eq!(b.raw_current(&id).unwrap(), "a=1\nb=2\nc=3\n");
    // Two undos peel the two structural edits one at a time.
    assert_eq!(b.undo(&id).unwrap().text, "a=1\nb=2\n");
    assert_eq!(b.undo(&id).unwrap().text, "a=1\n");
    assert!(b.undo(&id).is_err()); // nothing left
}

/// Encoding label + BOM round-trip through the snapshot (FROZEN F16
/// parity at the generic level).
#[test]
fn encoding_is_remembered_per_doc() {
    let b = backend();
    let enc = EncodingInfo { label: "windows-1252".into(), had_bom: true };
    let parsed = block_on(b.parse("a=1\n".into(), None, enc.clone())).unwrap();
    let got = b.get_encoding(&parsed.doc_id).unwrap();
    assert_eq!(got.label, "windows-1252");
    assert!(got.had_bom);
}
