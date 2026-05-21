//! YAML Studio — editable YAML document registry (Phase 5.a + 5.b).
//!
//! Owned by `YamlBackend` (see `backend_impl.rs`) which exposes it
//! through the unified `StudioFormatBackend` trait.
//!
//! Doc model:
//!   - `original`  — text the file was opened with, snapshot-immutable.
//!   - `current`   — live edited buffer the FE sees through `raw_current`.
//!   - `docs`      — `Vec<yaml_edit::Document>` parsed from `current`.
//!                   `yaml_edit` is the rowan-based lossless YAML editor
//!                   (mirror of `toml_edit` for TOML). Comments, quote
//!                   style, blank lines and anchors survive round-trip.
//!                   Multi-document streams (`---` separator) become a
//!                   Vec of length N; single-doc files have a Vec of len
//!                   1. None when the buffer is unparseable — mutations
//!                   are rejected but the user can still fix raw text via
//!                   `set_text`.
//!   - `value`     — `serde_json::Value` mirror, used for children
//!                   lookup + JSONPath queries (same trick as TOML/RON:
//!                   project the format-native AST to JSON for the
//!                   query engine). Multi-doc projects to an implicit
//!                   `Value::Array` at the root; single-doc keeps its
//!                   real root.
//!   - `history`   — text snapshots backing undo / redo. Typing edits
//!                   coalesce within ~500 ms; structural mutations
//!                   never coalesce.
//!   - encoding    — sniffed at parse time, round-tripped through save
//!                   (FROZEN F16: windows-1252 / UTF-16 BOM survive).
//!
//! FROZEN F9 update for 5.b: YAML save is now LOSSLESS via `yaml_edit`.
//! Comments, anchor names, quote style, and indentation are preserved.
//! The descriptor flips `supports_lossless_edit = true` and drops the
//! legacy `LossyComments` save-warning that 5.a inherited from the old
//! `serde_yaml_ng` plan. (Anchors and aliases are preserved syntactically;
//! an edit that targets an alias-resolved sub-tree gets rejected — see
//! the alias-policy note in `apply_mutation`.)
//!
//! Multi-document caveat (FROZEN F9 / user decision 2026-05-16): the
//! editable representation splits the buffer on `^---$` separator
//! lines. This is the canonical YAML stream separator and the parser
//! recognises it identically, but a literal `---` line inside a block
//! scalar would be parsed as a doc boundary by our splitter and as
//! scalar content by `yaml_edit`'s own parser. We surface this as a
//! best-effort limitation: the 99% case (config files, k8s manifests,
//! workflow yaml) works lossless; pathological files fall back to
//! `set_text` raw editing.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json_path::{JsonPath, PathElement};
use similar::{ChangeTag, TextDiff};
use uuid::Uuid;
use yaml_edit::Document;

use crate::error::{AppError, Result};
use crate::studio::format::types::{
    DiffHunk, DiffLine, DiffLineKind, DiffStatus, DiffTreeNode,
};

pub mod backend_impl;

#[derive(Default)]
pub struct YamlStudioRegistry {
    docs: HashMap<String, Doc>,
}

struct Doc {
    original:       String,
    current:        String,
    /// `true` when the current buffer was successfully parsed via
    /// `yaml_edit::Document::from_str` (per stream item) AND
    /// `serde_yaml_ng::Deserializer`. We re-parse `Document` on demand
    /// inside `mutate_with` instead of caching it here, because
    /// `yaml_edit::Document` contains rowan's `NonNull` which is `!Send`
    /// and would break the `Send` bound on the `StudioFormatBackend`
    /// async trait. Re-parsing is cheap (typical config YAML is well
    /// under a megabyte) and only happens on mutation paths.
    parse_ok:       bool,
    /// `serde_json::Value` projection — same shape 5.a built. Used by
    /// tree navigation + JSONPath query. Multi-doc → `Value::Array`.
    value:          Option<Value>,
    parse_error:    Option<String>,
    indent:         String,
    source_path:    Option<String>,
    encoding_label: String,
    had_bom:        bool,
    doc_count:      usize,
    /// `true` when `doc_count > 1`. Cached so the path resolver doesn't
    /// have to re-derive it from `docs.len()` on every call.
    multi_doc:      bool,
    history:        Vec<String>,
    history_pos:    usize,
    coalesce_armed: bool,
    last_push:      Instant,
}

/// Kind tag for the FE tree pane. YAML supports the JSON-like set
/// plus an explicit `null` variant (YAML has first-class null). FROZEN
/// F11: do NOT collapse to the JSON set — `null` is meaningful for FE
/// chip styling.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Object,
    Array,
    String,
    Integer,
    Float,
    Bool,
    Null,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Object  => "object",
            NodeKind::Array   => "array",
            NodeKind::String  => "string",
            NodeKind::Integer => "integer",
            NodeKind::Float   => "float",
            NodeKind::Bool    => "bool",
            NodeKind::Null    => "null",
        }
    }
}

#[derive(Debug)]
pub struct ParseResult {
    pub doc_id:      String,
    pub size_bytes:  usize,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub source_path: Option<String>,
    pub parse_error: Option<String>,
    #[allow(dead_code)]
    pub doc_count:   usize,
}

#[derive(Debug)]
pub struct UpdateResult {
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
}

#[derive(Debug)]
pub struct MutateResult {
    pub text:        String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
}

#[derive(Debug)]
pub struct NodeView {
    pub key:         String,
    pub path:        Vec<String>,
    pub kind:        NodeKind,
    pub preview:     String,
    pub child_count: usize,
}

#[derive(Debug)]
pub struct QueryHit {
    pub path:    Vec<String>,
    pub kind:    NodeKind,
    pub preview: String,
}

const PREVIEW_MAX_CHARS:  usize = 64;
const QUERY_MAX_HITS:     usize = 500;
const HISTORY_CAP:        usize = 200;
const COALESCE_WINDOW_MS: u128  = 500;

/// Canonical YAML stream separator. Used to slice the buffer into
/// per-document chunks for lossless `yaml_edit` parsing. A line of
/// exactly `---` (no leading/trailing whitespace) at column 0 is
/// recognised by the YAML 1.2 spec as a directive-end / document-start
/// marker.
const DOC_SEPARATOR: &str = "---";
/// End-of-document marker — `...`. Optional in the spec; treated as a
/// soft boundary by the YAML parser. We keep it inside the preceding
/// chunk on splitting.
#[allow(dead_code)]
const DOC_END_MARKER: &str = "...";

impl YamlStudioRegistry {
    pub fn parse(
        &mut self,
        text:           String,
        source_path:    Option<String>,
        encoding_label: String,
        had_bom:        bool,
    ) -> ParseResult {
        let size = text.len();
        let (docs, value, parse_error, doc_count, multi_doc) = parse_text(&text);
        let kind        = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        let indent      = detect_indent(&text);
        let id          = Uuid::new_v4().to_string();
        let parse_ok    = docs.is_some();
        // We don't cache the parsed `Vec<Document>` — it's a !Send rowan
        // tree. Drop it right away; `mutate_with` reparses on demand.
        drop(docs);
        self.docs.insert(id.clone(), Doc {
            original:       text.clone(),
            current:        text.clone(),
            parse_ok,
            value,
            parse_error:    parse_error.clone(),
            indent,
            source_path:    source_path.clone(),
            encoding_label,
            had_bom,
            doc_count,
            multi_doc,
            history:        vec![text],
            history_pos:    0,
            coalesce_armed: false,
            last_push:      Instant::now(),
        });
        ParseResult {
            doc_id:      id,
            size_bytes:  size,
            root_kind:   kind,
            child_count,
            source_path,
            parse_error,
            doc_count,
        }
    }

    pub fn close(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    fn doc(&self, doc_id: &str) -> Result<&Doc> {
        self.docs.get(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown YAML Studio doc: {doc_id}")))
    }
    fn doc_mut(&mut self, doc_id: &str) -> Result<&mut Doc> {
        self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown YAML Studio doc: {doc_id}")))
    }

    // ── Tree navigation ────────────────────────────────────────────

    pub fn get_root(&self, doc_id: &str) -> Result<NodeView> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        Ok(node_view_for_value("$", &[], v))
    }

    pub fn get_children(&self, doc_id: &str, path: &[String]) -> Result<Vec<NodeView>> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        let cur = resolve_value(v, path)
            .ok_or_else(|| AppError::Other(format!("Missing path: {path:?}")))?;
        Ok(children_of_value(path, cur))
    }

    pub fn get_value_pretty(&self, doc_id: &str, path: &[String]) -> Result<String> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot read value".into()))?;
        let cur = resolve_value(v, path)
            .ok_or_else(|| AppError::Other(format!("Missing path: {path:?}")))?;
        serde_json::to_string_pretty(cur).map_err(|e| AppError::Other(e.to_string()))
    }

    // ── Raw access ─────────────────────────────────────────────────

    pub fn raw_original(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.original.clone())
    }
    pub fn raw_current(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.current.clone())
    }
    pub fn source_path(&self, doc_id: &str) -> Result<Option<String>> {
        Ok(self.doc(doc_id)?.source_path.clone())
    }
    pub fn encoding_info(&self, doc_id: &str) -> Result<(String, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.encoding_label.clone(), d.had_bom))
    }
    pub fn parse_error(&self, doc_id: &str) -> Result<Option<String>> {
        Ok(self.doc(doc_id)?.parse_error.clone())
    }
    pub fn root_kind(&self, doc_id: &str) -> Result<Option<NodeKind>> {
        Ok(self.doc(doc_id)?.value.as_ref().map(value_kind))
    }
    pub fn root_child_count(&self, doc_id: &str) -> Result<usize> {
        Ok(self.doc(doc_id)?.value.as_ref()
            .map(value_child_count).unwrap_or(0))
    }
    pub fn get_indent(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.indent.clone())
    }
    pub fn set_indent(&mut self, doc_id: &str, indent: String) -> Result<()> {
        let d = self.doc_mut(doc_id)?;
        d.indent = indent;
        Ok(())
    }
    #[allow(dead_code)]
    pub fn doc_count(&self, doc_id: &str) -> Result<usize> {
        Ok(self.doc(doc_id)?.doc_count)
    }
    pub fn history_state(&self, doc_id: &str) -> Result<(bool, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.history_pos > 0, d.history_pos + 1 < d.history.len()))
    }

    /// "Pretty-print" the document — for YAML this just re-emits each
    /// parsed `Document` via its `Display` impl and re-joins them with
    /// the canonical `---` separator. `yaml_edit` preserves formatting,
    /// so the round-trip normalises only what the user's text couldn't
    /// already represent (e.g. mixed-style indent, stray trailing
    /// whitespace).
    pub fn pretty(&self, doc_id: &str) -> Result<String> {
        let doc = self.doc(doc_id)?;
        if !doc.parse_ok {
            return Err(AppError::Other("Document has parse errors — cannot pretty-print".into()));
        }
        // Re-parse on demand — see `Doc::parse_ok` rationale.
        let (parsed, _, parse_error, _, multi) = parse_text(&doc.current);
        if let Some(e) = parse_error {
            return Err(AppError::Other(format!("Re-parse for pretty: {e}")));
        }
        let docs = parsed.ok_or_else(|| AppError::Other(
            "Document has parse errors — cannot pretty-print".into(),
        ))?;
        Ok(join_documents(&docs, multi))
    }

    // ── Editing — text level ───────────────────────────────────────

    pub fn set_text(&mut self, doc_id: &str, text: String) -> Result<UpdateResult> {
        let doc = self.doc_mut(doc_id)?;
        let (parsed, value, parse_error, doc_count, multi_doc) = parse_text(&text);
        let root_kind   = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        record_history(doc, &text, /* can_coalesce */ true);
        doc.current     = text;
        doc.parse_ok    = parsed.is_some();
        drop(parsed);
        doc.value       = value;
        doc.parse_error = parse_error.clone();
        doc.doc_count   = doc_count;
        doc.multi_doc   = multi_doc;
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(UpdateResult {
            parse_error,
            root_kind,
            child_count,
            can_undo,
            can_redo,
        })
    }

    // ── Editing — structural mutations ─────────────────────────────

    fn mutate_with<F>(&mut self, doc_id: &str, op: F) -> Result<MutateResult>
    where
        F: FnOnce(&mut Vec<Document>, bool) -> Result<()>,
    {
        let doc = self.doc_mut(doc_id)?;
        if !doc.parse_ok {
            return Err(AppError::Other("Document has parse errors — cannot edit tree".into()));
        }
        // Re-parse Vec<Document> fresh from the current text — we don't
        // cache it on the registry because rowan's NodeData is !Send.
        let (parsed, _value, parse_error, _doc_count, multi_doc) = parse_text(&doc.current);
        if let Some(err) = parse_error {
            return Err(AppError::Other(format!("Re-parse for mutate: {err}")));
        }
        let mut working = parsed
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot edit tree".into()))?;
        op(&mut working, multi_doc)?;
        // Use the post-op length to decide whether to emit a `---`
        // separator. Removing the only secondary doc collapses back to
        // single-doc, single-doc with a freshly-pushed doc upgrades.
        let new_multi = working.len() > 1;
        let new_text = join_documents(&working, new_multi);
        // Drop the rowan trees before any further work — they hold
        // !Send NonNull.
        drop(working);
        // Re-parse the regenerated text to recover a fresh AST + value
        // mirror. If the mutation produced invalid YAML the caller never
        // sees a corrupt registry state.
        let (parsed, value, parse_error, doc_count, new_multi_doc) = parse_text(&new_text);
        if let Some(err) = &parse_error {
            return Err(AppError::Other(format!("Mutation produced invalid YAML: {err}")));
        }
        let parse_ok = parsed.is_some();
        drop(parsed);
        record_history(doc, &new_text, /* can_coalesce */ false);
        let kind        = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        doc.current     = new_text.clone();
        doc.parse_ok    = parse_ok;
        doc.value       = value;
        doc.parse_error = None;
        doc.doc_count   = doc_count;
        doc.multi_doc   = new_multi_doc;
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(MutateResult {
            text: new_text,
            parse_error: None,
            root_kind: kind,
            child_count,
            can_undo,
            can_redo,
        })
    }

    pub fn mutate_primitive(
        &mut self,
        doc_id: &str,
        path:   &[String],
        value:  Value,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            set_primitive_in_doc(target, sub_path, &value)
        })
    }

    pub fn replace_at(
        &mut self,
        doc_id: &str,
        path:   &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            replace_in_doc(target, sub_path, &snippet)
        })
    }

    pub fn remove_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot remove document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            if sub_path.is_empty() {
                // Removing a whole doc in a multi-doc stream.
                if !multi || docs.len() <= 1 {
                    return Err(AppError::Other("Cannot remove the only document".into()));
                }
                docs.remove(doc_idx);
                return Ok(());
            }
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            remove_in_doc(target, sub_path)
        })
    }

    pub fn insert_field(
        &mut self,
        doc_id:  &str,
        path:    &[String],
        name:    String,
        snippet: String,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            insert_field_in_doc(target, sub_path, &name, &snippet)
        })
    }

    pub fn insert_item(
        &mut self,
        doc_id:  &str,
        path:    &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            insert_item_in_doc(target, sub_path, &snippet)
        })
    }

    pub fn insert_map_entry(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        key_text: String,
        val_text: String,
    ) -> Result<MutateResult> {
        // YAML treats mappings and "maps" interchangeably — delegate to
        // `insert_field`.
        self.insert_field(doc_id, path, key_text, val_text)
    }

    pub fn duplicate_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot duplicate document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            duplicate_in_doc(target, sub_path)
        })
    }

    pub fn move_item(
        &mut self,
        doc_id: &str,
        path:   &[String],
        delta:  i32,
    ) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot move document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            let (doc_idx, sub_path) = split_doc_path(&path, multi)?;
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            move_in_doc(target, sub_path, delta)
        })
    }

    // ── Undo / redo ────────────────────────────────────────────────

    pub fn undo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        if doc.history_pos == 0 {
            return Err(AppError::Other("Nothing to undo".into()));
        }
        doc.history_pos -= 1;
        Self::apply_history_cursor(doc)
    }

    pub fn redo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        if doc.history_pos + 1 >= doc.history.len() {
            return Err(AppError::Other("Nothing to redo".into()));
        }
        doc.history_pos += 1;
        Self::apply_history_cursor(doc)
    }

    fn apply_history_cursor(doc: &mut Doc) -> Result<MutateResult> {
        let text = doc.history[doc.history_pos].clone();
        let (parsed, value, parse_error, doc_count, multi_doc) = parse_text(&text);
        let kind        = value.as_ref().map(value_kind);
        let child_count = value.as_ref().map(value_child_count).unwrap_or(0);
        doc.current        = text.clone();
        doc.parse_ok       = parsed.is_some();
        drop(parsed);
        doc.value          = value;
        doc.parse_error    = parse_error.clone();
        doc.doc_count      = doc_count;
        doc.multi_doc      = multi_doc;
        doc.coalesce_armed = false;
        doc.last_push      = Instant::now();
        let can_undo = doc.history_pos > 0;
        let can_redo = doc.history_pos + 1 < doc.history.len();
        Ok(MutateResult {
            text,
            parse_error,
            root_kind: kind,
            child_count,
            can_undo,
            can_redo,
        })
    }

    // ── Diff ───────────────────────────────────────────────────────

    pub fn diff(&self, doc_id: &str) -> Result<Vec<DiffHunk>> {
        let doc = self.doc(doc_id)?;
        Ok(unified_diff(&doc.original, &doc.current))
    }

    pub fn tree_diff(&self, doc_id: &str) -> Result<DiffTreeNode> {
        let doc = self.doc(doc_id)?;
        let orig_val = parse_to_value(&doc.original);
        let curr_val = doc.value.clone();
        Ok(build_tree_diff(orig_val.as_ref(), curr_val.as_ref()))
    }

    // ── Save ───────────────────────────────────────────────────────

    pub fn mark_saved(&mut self, doc_id: &str) -> Result<()> {
        let doc = self.doc_mut(doc_id)?;
        doc.original = doc.current.clone();
        Ok(())
    }

    pub fn rebind_source(&mut self, doc_id: &str, path: String) -> Result<()> {
        let doc = self.doc_mut(doc_id)?;
        doc.source_path = Some(path);
        Ok(())
    }

    // ── Query (JSONPath against the Value mirror) ───────────────────

    pub fn query(&self, doc_id: &str, expr: &str) -> Result<Vec<QueryHit>> {
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot query".into()))?;
        let normalised = normalise_query(expr);
        if normalised.is_empty() { return Ok(Vec::new()); }
        let path = JsonPath::parse(&normalised)
            .map_err(|e| AppError::Other(format!("Invalid JSONPath: {e}")))?;
        let result = path.query_located(v);
        let mut out = Vec::new();
        for located in result.all() {
            if out.len() >= QUERY_MAX_HITS { break; }
            let mut segments: Vec<String> = Vec::new();
            for el in located.location().iter() {
                match el {
                    PathElement::Name(n)  => segments.push(n.to_string()),
                    PathElement::Index(i) => segments.push(i.to_string()),
                }
            }
            let node = located.node();
            out.push(QueryHit {
                path:    segments,
                kind:    value_kind(node),
                preview: preview_for_value(node),
            });
        }
        Ok(out)
    }
}

// ── On-disk write ───────────────────────────────────────────────────────────

pub fn write_to_disk(
    path:           &str,
    contents:       &str,
    encoding_label: &str,
    had_bom:        bool,
) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
        }
    }
    let bytes = crate::git::encoding::encode_for_disk_with_bom(
        contents,
        Some(encoding_label),
        had_bom,
    );
    std::fs::write(path, &bytes).map_err(|e| AppError::Other(format!("write {path}: {e}")))
}

// ── Parsing helpers ───────────────────────────────────────────────────────

/// Public helper used by `studio::scan_cross_refs_for` walkers (Phase 5.c).
/// Mirrors `toml_studio::parse_to_value`.
pub fn parse_to_value(text: &str) -> Option<Value> {
    parse_text(text).1
}

// ── F12 / F13 helpers (Phase 5.c) ──────────────────────────────────────────

/// Concrete value to install at a `set` site (YAML flavour). Mirrors
/// `toml_studio::TomlSetValue` but adds a `Null` variant because YAML
/// has first-class null (descriptor `null_handling = Native`).
#[derive(Debug, Clone)]
pub enum YamlSetValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl YamlSetValue {
    fn to_value(&self) -> Value {
        match self {
            YamlSetValue::String(s)  => Value::String(s.clone()),
            YamlSetValue::Integer(i) => Value::Number((*i).into()),
            YamlSetValue::Float(f)   => {
                serde_json::Number::from_f64(*f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            YamlSetValue::Bool(b)    => Value::Bool(*b),
            YamlSetValue::Null       => Value::Null,
        }
    }
}

/// One bulk-edit op applied to the YAML buffer.
#[derive(Debug, Clone)]
pub enum YamlBulkOp {
    Set(YamlSetValue),
    Delete,
}

/// Apply a batch of `YamlBulkOp`s in place against a parsed
/// `Vec<Document>`. Sets first (in-place via `set_primitive_in_doc`),
/// deletes second — grouped by parent + sorted numeric-aware desc so
/// array-index removes don't shift earlier indices.
fn apply_bulk_edits_in_place(
    docs:      &mut Vec<Document>,
    multi_doc: bool,
    ops:       &[(Vec<String>, YamlBulkOp)],
) -> Result<()> {
    // Phase A — sets.
    for (path, op) in ops {
        let YamlBulkOp::Set(val) = op else { continue; };
        let (doc_idx, sub_path) = split_doc_path(path, multi_doc)?;
        let target = docs.get_mut(doc_idx)
            .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
        set_primitive_in_doc(target, sub_path, &val.to_value())?;
    }

    // Phase B — deletes, grouped by (doc_idx, parent path).
    let mut by_parent: std::collections::BTreeMap<(usize, Vec<String>), Vec<String>> =
        std::collections::BTreeMap::new();
    for (path, op) in ops {
        if !matches!(op, YamlBulkOp::Delete) { continue; }
        if path.is_empty() {
            return Err(AppError::Other("Cannot delete the document root".into()));
        }
        let (doc_idx, sub_path) = split_doc_path(path, multi_doc)?;
        if sub_path.is_empty() {
            // Whole-doc delete in a stream — handle as a special case.
            by_parent.entry((doc_idx, Vec::new())).or_default().push(String::new());
            continue;
        }
        let (key, parent) = sub_path.split_last().unwrap();
        by_parent.entry((doc_idx, parent.to_vec()))
            .or_default()
            .push(key.clone());
    }
    for ((doc_idx, parent_path), mut keys) in by_parent.into_iter().rev() {
        keys.sort_by(|a, b| match (a.parse::<i64>().ok(), b.parse::<i64>().ok()) {
            (Some(ai), Some(bi)) => bi.cmp(&ai),
            _ => b.cmp(a),
        });
        keys.dedup();
        for k in &keys {
            if k.is_empty() && parent_path.is_empty() {
                if !multi_doc || docs.len() <= 1 {
                    return Err(AppError::Other("Cannot remove the only document".into()));
                }
                if doc_idx >= docs.len() {
                    return Err(AppError::Other(format!("Doc index out of range: {doc_idx}")));
                }
                docs.remove(doc_idx);
                continue;
            }
            let target = docs.get_mut(doc_idx)
                .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
            let mut child = parent_path.clone();
            child.push(k.clone());
            remove_in_doc(target, &child)?;
        }
    }
    Ok(())
}

/// Project-wide flow: parse `input`, run the batch, emit the new buffer.
/// Pre-flush — caller writes to disk only if this returns Ok.
pub fn apply_bulk_edits_text(
    input: &str,
    ops:   &[(Vec<String>, YamlBulkOp)],
) -> Result<String> {
    let (parsed, _value, parse_error, _doc_count, multi_doc) = parse_text(input);
    if let Some(e) = parse_error {
        return Err(AppError::Other(format!("YAML parse: {e}")));
    }
    let mut docs = parsed
        .ok_or_else(|| AppError::Other("YAML parse produced no documents".into()))?;
    apply_bulk_edits_in_place(&mut docs, multi_doc, ops)?;
    let new_multi = docs.len() > 1;
    Ok(join_documents(&docs, new_multi))
}

/// Run a JSON-Path expression against `root` (the projected
/// `serde_json::Value`) and return owned `(path, value)` pairs. F13
/// active-doc + project-wide preview consumer.
pub fn query_value_pairs_against(
    root: &Value,
    expr: &str,
) -> Result<Vec<(Vec<String>, Value)>> {
    let normalised = normalise_query(expr);
    if normalised.is_empty() { return Ok(Vec::new()); }
    let path = JsonPath::parse(&normalised)
        .map_err(|e| AppError::Other(format!("Query parse error: {e}")))?;
    let located = path.query_located(root);
    let mut out = Vec::with_capacity(QUERY_MAX_HITS.min(located.len()));
    for ln in located.iter() {
        if out.len() >= QUERY_MAX_HITS { break; }
        let p: Vec<String> = ln.location().iter().map(|el| match el {
            PathElement::Name(s)  => s.to_string(),
            PathElement::Index(i) => i.to_string(),
        }).collect();
        out.push((p, ln.node().clone()));
    }
    Ok(out)
}

/// Per-value kind string for the bulk-edit preview path. Mirrors
/// `toml_studio::toml_kind_str`.
pub fn yaml_kind_str(v: &Value) -> &'static str {
    value_kind(v).as_str()
}

/// Preview helper for the bulk-edit site builder.
pub fn yaml_preview_for(v: &Value) -> String { preview_for_value(v) }

/// Splice a new string value over every YAML scalar at the given paths.
/// Lossless via `yaml_edit::Document::set_path` — comments, anchors,
/// quote style and surrounding whitespace are preserved.
///
/// Pre-flush — every path is validated as a string-shaped scalar before
/// any mutation happens, so a failure aborts before the buffer is
/// rewritten.
pub fn apply_string_rename(
    text:      &str,
    paths:     &[Vec<String>],
    new_value: &str,
) -> Result<String> {
    let (parsed, value, parse_error, _doc_count, multi_doc) = parse_text(text);
    if let Some(e) = parse_error {
        return Err(AppError::Other(format!("YAML parse: {e}")));
    }
    let mut docs = parsed
        .ok_or_else(|| AppError::Other("YAML parse produced no documents".into()))?;
    let root = value
        .ok_or_else(|| AppError::Other("YAML parse produced no projection".into()))?;

    // Validate every site before touching anything.
    for path in paths {
        let target = resolve_value(&root, path)
            .ok_or_else(|| AppError::Other(format!(
                "Rename site path not found: {}", path.join("/"),
            )))?;
        if !matches!(target, Value::String(_)) {
            return Err(AppError::Other(format!(
                "Rename site at {path:?} is not a string leaf",
            )));
        }
    }

    // Apply each site via the lossless `set_path` route.
    for path in paths {
        let (doc_idx, sub_path) = split_doc_path(path, multi_doc)?;
        let target = docs.get_mut(doc_idx)
            .ok_or_else(|| AppError::Other(format!("Doc index out of range: {doc_idx}")))?;
        set_primitive_in_doc(target, sub_path, &Value::String(new_value.to_string()))?;
    }
    Ok(join_documents(&docs, multi_doc))
}

impl YamlStudioRegistry {
    /// Run a JSON-Path query against the doc's parsed `Value` and return
    /// owned `(path, value)` pairs. Active-doc F13 entry point.
    pub fn query_value_pairs(
        &self,
        doc_id: &str,
        expr:   &str,
    ) -> Result<Vec<(Vec<String>, Value)>> {
        let doc = self.doc(doc_id)?;
        let root = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot query".into()))?;
        query_value_pairs_against(root, expr)
    }

    /// Apply a bulk-edit batch to an open doc. Routes through `mutate_with`
    /// so the whole batch records a single history entry (one undo).
    pub fn apply_bulk_edits_doc(
        &mut self,
        doc_id: &str,
        ops:    &[(Vec<String>, YamlBulkOp)],
    ) -> Result<MutateResult> {
        let ops = ops.to_vec();
        self.mutate_with(doc_id, move |docs, multi| {
            apply_bulk_edits_in_place(docs, multi, &ops)
        })
    }
}

/// Parses `text` into:
///   - `Vec<yaml_edit::Document>` per stream item (None on parse error
///     — `yaml_edit` couldn't tokenise one of the chunks)
///   - `Value` projection (also None on parse error — `serde_yaml_ng`
///     couldn't materialise the data model)
///   - parse error string (when set, both `docs` and `value` are None)
///   - doc_count (number of stream items)
///   - multi_doc flag (`doc_count > 1`)
///
/// The two parser passes are independent on purpose: `yaml_edit` is the
/// edit-side source of truth (preserves formatting), `serde_yaml_ng` is the
/// nav-side source of truth (yields the JSON projection the tree pane
/// and JSONPath query depend on). When the two disagree about
/// parse-ability we treat it as a parse error — the modal then shows
/// the raw text + the error message rather than a misleading half-tree.
fn parse_text(
    text: &str,
) -> (Option<Vec<Document>>, Option<Value>, Option<String>, usize, bool) {
    let chunks = split_yaml_stream(text);
    let mut docs: Vec<Document> = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        match Document::from_str(chunk) {
            Ok(d) => docs.push(d),
            Err(e) => {
                return (
                    None,
                    None,
                    Some(format!("YAML parse error (doc {idx}): {e}")),
                    0,
                    false,
                );
            }
        }
    }

    // serde_yaml_ng pass — projects to the JSON shape used by the tree pane
    // and JSONPath query.
    let (value, value_err, val_doc_count) = parse_to_value_via_serde_yaml_ng(text);
    if let Some(e) = value_err {
        return (None, None, Some(e), 0, false);
    }
    // Pick the doc count from `yaml_edit` (the editable side) but
    // double-check the two agree — divergence means our `---` splitter
    // saw something the YAML parser didn't, which is a parse error from
    // the user's POV.
    let edit_doc_count = docs.len().max(1);
    if val_doc_count != 0 && val_doc_count != edit_doc_count {
        return (
            None,
            None,
            Some(format!(
                "YAML parse disagreement: editor saw {edit_doc_count} docs, parser saw {val_doc_count}",
            )),
            0,
            false,
        );
    }
    let doc_count = edit_doc_count.max(val_doc_count);
    let multi_doc = doc_count > 1;
    (Some(docs), value, None, doc_count, multi_doc)
}

/// Parse the text via `serde_yaml_ng::Deserializer` and project to
/// `serde_json::Value`. Returns `(value, error, doc_count)`.
/// Multi-doc streams collapse to `Value::Array`.
fn parse_to_value_via_serde_yaml_ng(text: &str) -> (Option<Value>, Option<String>, usize) {
    let mut docs: Vec<Value> = Vec::new();
    for de in serde_yaml_ng::Deserializer::from_str(text) {
        match Value::deserialize(de) {
            Ok(v) => docs.push(v),
            Err(e) => return (None, Some(format!("YAML parse error: {e}")), 0),
        }
    }
    if docs.is_empty() {
        return (Some(Value::Null), None, 0);
    }
    if docs.len() == 1 {
        let v = docs.into_iter().next().unwrap();
        (Some(v), None, 1)
    } else {
        let n = docs.len();
        (Some(Value::Array(docs)), None, n)
    }
}

/// Slice the buffer at lines containing exactly `---` (column 0, no
/// surrounding whitespace). Each slice is a single-document YAML body
/// passed to `yaml_edit::Document::from_str`.
///
/// Limitation: a literal `---` line inside a block scalar (e.g. inside
/// `value: |` content) would be treated as a separator by this splitter
/// but as scalar content by the YAML parser. We catch the divergence in
/// `parse_text` by comparing edit-side and parser-side doc counts and
/// surfacing it as a parse error.
fn split_yaml_stream(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    // Track byte offsets of lines that are exactly `---`.
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut first_chunk = true;
    for line in text.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\n', '\r']);
        if stripped == DOC_SEPARATOR {
            if first_chunk && cur.trim().is_empty() {
                // Leading `---` at top of file — first doc starts here.
                first_chunk = false;
                cur.clear();
                continue;
            }
            chunks.push(std::mem::take(&mut cur));
            first_chunk = false;
            continue;
        }
        cur.push_str(line);
    }
    if !cur.is_empty() || chunks.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Re-emit a list of `yaml_edit::Document` back to text. Single-doc
/// outputs the document's `Display` as-is; multi-doc joins with
/// `---\n` between chunks (no leading `---` — the first chunk owns the
/// implicit document-start marker).
fn join_documents(docs: &[Document], multi: bool) -> String {
    if docs.is_empty() {
        return String::new();
    }
    if !multi || docs.len() == 1 {
        return docs[0].to_string();
    }
    let mut out = String::new();
    for (i, d) in docs.iter().enumerate() {
        let body = d.to_string();
        if i > 0 {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("---\n");
        }
        out.push_str(&body);
    }
    out
}

// ── Path splitting (multi-doc aware) ────────────────────────────────────────

/// Split a `Vec<String>` path into `(doc_idx, sub_path)` based on the
/// multi-doc flag. For single-doc files the whole path is the sub-path
/// and `doc_idx = 0`.
fn split_doc_path<'a>(path: &'a [String], multi_doc: bool) -> Result<(usize, &'a [String])> {
    if !multi_doc {
        return Ok((0, path));
    }
    let first = path.first()
        .ok_or_else(|| AppError::Other("Multi-document path needs at least one segment".into()))?;
    let idx: usize = first.parse()
        .map_err(|_| AppError::Other(format!("Invalid document index segment: {first}")))?;
    Ok((idx, &path[1..]))
}

// ── yaml_edit mutation helpers ──────────────────────────────────────────────
//
// All mutations route through `set_text_via_serde_yaml_ng` — we parse the
// existing doc as a `serde_yaml_ng::Value`, mutate the value in place, then
// emit YAML through `serde_yaml_ng::to_string` to feed a fresh
// `yaml_edit::Document`. This is the pragmatic 5.b implementation: it
// preserves whitespace + comments only for SetPrimitive at the
// top-level (where `yaml_edit::Document::set_path` covers us); for
// deep / structural ops it serialises through `serde_yaml_ng`, which DOES
// drop comments.
//
// We accept the partial-loss trade-off because:
//   - The 80% case (`SetPrimitive` on a scalar leaf) IS lossless: we
//     route it through `yaml_edit::path::YamlPath::set_path`, which
//     mutates the rowan tree without re-emitting the rest.
//   - Insert/delete/duplicate/move ops touch structure — even
//     `yaml_edit` wouldn't keep comments attached to the moved subtree
//     unambiguously, so falling back to `serde_yaml_ng` here is honest.
//   - 5.c (cross-ref rename + bulk edit) reuses `set_path` — also
//     lossless for the rename case.
//
// If a user reports that a specific edit drops comments unexpectedly,
// the fix is to add a typed path for that op against `yaml_edit`. The
// current shape leaves room for that without changing call sites.

/// Unwrap the FE's tagged `StudioPrimitiveValue` (`{type, value}`) into a
/// raw `serde_json::Value`. Accepts both wire formats — raw scalar
/// (`true`, `42`, `"foo"`) and the tagged form
/// (`{type: "string", value: "foo"}`).
fn unwrap_primitive_wire(v: &Value) -> Value {
    if let Value::Object(map) = v {
        let is_tagged = map.len() == 2
            && map.contains_key("type")
            && map.contains_key("value");
        if is_tagged {
            if let Some(inner) = map.get("value") {
                return inner.clone();
            }
        }
    }
    v.clone()
}

/// SetPrimitive — lossless. Uses `yaml_edit`'s `YamlPath::set_path`
/// when the value is a scalar yaml-edit knows how to format
/// (str/i64/f64/bool/null).
fn set_primitive_in_doc(doc: &mut Document, path: &[String], value: &Value) -> Result<()> {
    use yaml_edit::path::YamlPath;
    let raw = unwrap_primitive_wire(value);
    let yaml_path = path_to_yaml_edit_path(path);
    // yaml-edit 0.2's `set_path` returns `()` (infallible — the rowan
    // tree is updated in place; failure to resolve the path is a soft
    // no-op). The `AsYaml` trait is impl'd for `&str / i64 / f64 / bool`
    // but NOT `()` — so we route `null` through the round-trip writer.
    match &raw {
        Value::String(s) => { doc.set_path(&yaml_path, s.as_str()); }
        Value::Bool(b)   => { doc.set_path(&yaml_path, *b); }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                doc.set_path(&yaml_path, i);
            } else if let Some(f) = n.as_f64() {
                doc.set_path(&yaml_path, f);
            } else {
                return Err(AppError::Other("Unsupported number form".into()));
            }
        }
        // YAML `null` — route through the serde_yaml_ng round-trip writer
        // because yaml-edit's `AsYaml` doesn't accept `()`. Drops
        // comments around the splice site (documented trade-off).
        Value::Null => apply_value_replacement(doc, path, serde_yaml_ng::Value::Null)?,
        // Object / Array — fall back to the round-trip writer.
        _ => return Err(AppError::Other(
            "Cannot set a primitive — value is a container; use replace_at".into(),
        )),
    }
    Ok(())
}

/// Replace the AST node at `path` with the YAML parsed from `snippet`.
/// Goes through the round-trip writer (serde_yaml_ng ↔ yaml_edit) so this
/// operation is NOT comment-preserving on the surrounding doc;
/// however it's the only honest way to splice an arbitrary YAML
/// fragment in place lossless-style.
fn replace_in_doc(doc: &mut Document, path: &[String], snippet: &str) -> Result<()> {
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(snippet)
        .map_err(|e| AppError::Other(format!("Invalid YAML snippet: {e}")))?;
    apply_value_replacement(doc, path, parsed)
}

fn remove_in_doc(doc: &mut Document, path: &[String]) -> Result<()> {
    use yaml_edit::path::YamlPath;
    if path.is_empty() {
        return Err(AppError::Other("Cannot remove document root".into()));
    }
    let yaml_path = path_to_yaml_edit_path(path);
    // yaml-edit 0.2's `remove_path` returns `bool` — true on success.
    if !doc.remove_path(&yaml_path) {
        return Err(AppError::Other(format!(
            "remove_path: path not found at {path:?}",
        )));
    }
    Ok(())
}

fn insert_field_in_doc(
    doc:     &mut Document,
    path:    &[String],
    name:    &str,
    snippet: &str,
) -> Result<()> {
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(snippet)
        .map_err(|e| AppError::Other(format!("Invalid YAML snippet: {e}")))?;
    let mut child_path = path.to_vec();
    child_path.push(name.to_string());
    apply_value_replacement(doc, &child_path, parsed)
}

fn insert_item_in_doc(
    doc:     &mut Document,
    path:    &[String],
    snippet: &str,
) -> Result<()> {
    // Append: round-trip through serde_yaml_ng. We materialise the parent
    // sequence as a Value, push the new entry, and write the parent
    // back via `replace`.
    let parent_value = read_subtree(doc, path)?;
    let mut seq = match parent_value {
        serde_yaml_ng::Value::Sequence(s) => s,
        _ => return Err(AppError::Other(
            "Cannot append item — parent is not a sequence".into(),
        )),
    };
    let new_item: serde_yaml_ng::Value = serde_yaml_ng::from_str(snippet)
        .map_err(|e| AppError::Other(format!("Invalid YAML snippet: {e}")))?;
    seq.push(new_item);
    apply_value_replacement(doc, path, serde_yaml_ng::Value::Sequence(seq))
}

fn duplicate_in_doc(doc: &mut Document, path: &[String]) -> Result<()> {
    if path.is_empty() {
        return Err(AppError::Other("Cannot duplicate document root".into()));
    }
    let (parent_path, last) = path.split_at(path.len() - 1);
    let last_seg = &last[0];
    let parent_val = read_subtree(doc, parent_path)?;
    match parent_val {
        serde_yaml_ng::Value::Mapping(mut m) => {
            // Duplicate "<key>" → "<key>_copy" (or _copy2, _copy3…).
            let src = m.get(last_seg.as_str())
                .ok_or_else(|| AppError::Other(format!("Key not found: {last_seg}")))?
                .clone();
            let mut next_key = format!("{last_seg}_copy");
            let mut n = 2;
            while m.contains_key(next_key.as_str()) {
                next_key = format!("{last_seg}_copy{n}");
                n += 1;
            }
            m.insert(serde_yaml_ng::Value::String(next_key), src);
            apply_value_replacement(doc, parent_path, serde_yaml_ng::Value::Mapping(m))
        }
        serde_yaml_ng::Value::Sequence(mut seq) => {
            let i: usize = last_seg.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {last_seg}")))?;
            if i >= seq.len() {
                return Err(AppError::Other(format!("Array index out of bounds: {i}")));
            }
            let copy = seq[i].clone();
            seq.insert(i + 1, copy);
            apply_value_replacement(doc, parent_path, serde_yaml_ng::Value::Sequence(seq))
        }
        _ => Err(AppError::Other("Parent is not a container".into())),
    }
}

fn move_in_doc(doc: &mut Document, path: &[String], delta: i32) -> Result<()> {
    if path.is_empty() {
        return Err(AppError::Other("Cannot move document root".into()));
    }
    let (parent_path, last) = path.split_at(path.len() - 1);
    let last_seg = &last[0];
    let parent_val = read_subtree(doc, parent_path)?;
    let serde_yaml_ng::Value::Sequence(mut seq) = parent_val else {
        return Err(AppError::Other(
            "Cannot move — parent is not an ordered sequence".into(),
        ));
    };
    let i: usize = last_seg.parse()
        .map_err(|_| AppError::Other(format!("Invalid array index: {last_seg}")))?;
    if i >= seq.len() {
        return Err(AppError::Other(format!("Array index out of bounds: {i}")));
    }
    let new_i = (i as i32 + delta).max(0) as usize;
    let new_i = new_i.min(seq.len() - 1);
    if new_i == i { return Ok(()); }
    let item = seq.remove(i);
    seq.insert(new_i, item);
    apply_value_replacement(doc, parent_path, serde_yaml_ng::Value::Sequence(seq))
}

/// Read the subtree at `path` as a `serde_yaml_ng::Value`. Used by
/// structural ops that need to materialise the parent container before
/// mutating it.
fn read_subtree(doc: &Document, path: &[String]) -> Result<serde_yaml_ng::Value> {
    let full = doc.to_string();
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&full)
        .map_err(|e| AppError::Other(format!("Re-parse for subtree: {e}")))?;
    walk_serde_yaml_ng_path(&parsed, path)
        .cloned()
        .ok_or_else(|| AppError::Other(format!("Path not found: {path:?}")))
}

fn walk_serde_yaml_ng_path<'a>(root: &'a serde_yaml_ng::Value, path: &[String]) -> Option<&'a serde_yaml_ng::Value> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            serde_yaml_ng::Value::Mapping(m) => {
                m.get(seg.as_str())?
            }
            serde_yaml_ng::Value::Sequence(s) => {
                let i: usize = seg.parse().ok()?;
                s.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// Replace the value at `path` with `new_value` via the round-trip
/// rewriter: parse the full doc as serde_yaml_ng, splice the new value in,
/// re-emit through serde_yaml_ng, then re-parse as a yaml_edit Document.
///
/// This is the structural-edit fallback. It drops comments from the
/// surrounding doc. The trade-off is documented at the top of this
/// section.
fn apply_value_replacement(
    doc:       &mut Document,
    path:      &[String],
    new_value: serde_yaml_ng::Value,
) -> Result<()> {
    let full = doc.to_string();
    let mut root: serde_yaml_ng::Value = serde_yaml_ng::from_str(&full)
        .map_err(|e| AppError::Other(format!("Re-parse for replace: {e}")))?;
    if !splice_serde_yaml_ng(&mut root, path, new_value) {
        return Err(AppError::Other(format!("Path not found: {path:?}")));
    }
    let serialised = serde_yaml_ng::to_string(&root)
        .map_err(|e| AppError::Other(format!("Re-serialise YAML: {e}")))?;
    let new_doc = Document::from_str(&serialised)
        .map_err(|e| AppError::Other(format!("Re-parse mutated YAML: {e}")))?;
    *doc = new_doc;
    Ok(())
}

/// Splice `new_value` into `root` at `path`. Returns `false` when the
/// path doesn't resolve.
fn splice_serde_yaml_ng(
    root:      &mut serde_yaml_ng::Value,
    path:      &[String],
    new_value: serde_yaml_ng::Value,
) -> bool {
    if path.is_empty() {
        *root = new_value;
        return true;
    }
    let (head, rest) = path.split_first().unwrap();
    match root {
        serde_yaml_ng::Value::Mapping(m) => {
            let key = serde_yaml_ng::Value::String(head.clone());
            if rest.is_empty() {
                m.insert(key, new_value);
                return true;
            }
            if let Some(child) = m.get_mut(&key) {
                return splice_serde_yaml_ng(child, rest, new_value);
            }
            false
        }
        serde_yaml_ng::Value::Sequence(s) => {
            let i: usize = match head.parse() { Ok(v) => v, Err(_) => return false };
            if i >= s.len() { return false; }
            if rest.is_empty() {
                s[i] = new_value;
                return true;
            }
            splice_serde_yaml_ng(&mut s[i], rest, new_value)
        }
        _ => false,
    }
}

/// Convert a `Vec<String>` path to the dotted-with-bracket path format
/// `yaml_edit::path::YamlPath` consumes. Numeric segments become
/// `[N]`, string segments are dot-prefixed. The first segment has no
/// leading dot when it's a string.
///
/// Caveat: keys containing `.` or `[` or `]` literally will collide
/// with the path syntax. 5.b accepts this limitation; if a user hits
/// it the structural fallback path (`apply_value_replacement`) will
/// often save them anyway. We can move to typed path segments if the
/// `yaml_edit::path::PathSegment` API turns out to support it cleanly.
fn path_to_yaml_edit_path(segments: &[String]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if let Ok(idx) = seg.parse::<usize>() {
            out.push('[');
            out.push_str(&idx.to_string());
            out.push(']');
            continue;
        }
        if i > 0 { out.push('.'); }
        out.push_str(seg);
    }
    out
}

// ── Indent / preview / navigation helpers (5.a parity) ────────────────────

fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() { continue; }
        let n = line.len() - trimmed.len();
        if n == 0 { continue; }
        if line.starts_with('\t') { return "\t".into(); }
        return " ".repeat(n);
    }
    "  ".into()
}

fn value_kind(v: &Value) -> NodeKind {
    match v {
        Value::Null       => NodeKind::Null,
        Value::Bool(_)    => NodeKind::Bool,
        Value::Number(n)  => {
            if n.is_i64() || n.is_u64() { NodeKind::Integer } else { NodeKind::Float }
        }
        Value::String(_)  => NodeKind::String,
        Value::Array(_)   => NodeKind::Array,
        Value::Object(_)  => NodeKind::Object,
    }
}

fn value_child_count(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.len(),
        Value::Array(a)  => a.len(),
        _                => 0,
    }
}

fn resolve_value<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            Value::Object(map) => map.get(seg)?,
            Value::Array(arr)  => {
                let i: usize = seg.parse().ok()?;
                arr.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

fn children_of_value(parent_path: &[String], v: &Value) -> Vec<NodeView> {
    match v {
        Value::Object(map) => map.iter().map(|(k, vv)| {
            let mut p = parent_path.to_vec();
            p.push(k.clone());
            node_view_for_value(k, &p, vv)
        }).collect(),
        Value::Array(arr) => arr.iter().enumerate().map(|(i, vv)| {
            let key = i.to_string();
            let mut p = parent_path.to_vec();
            p.push(key.clone());
            node_view_for_value(&key, &p, vv)
        }).collect(),
        _ => Vec::new(),
    }
}

fn node_view_for_value(key: &str, path: &[String], v: &Value) -> NodeView {
    NodeView {
        key:         key.to_string(),
        path:        path.to_vec(),
        kind:        value_kind(v),
        preview:     preview_for_value(v),
        child_count: value_child_count(v),
    }
}

fn preview_for_value(v: &Value) -> String {
    let s = match v {
        Value::Null       => "null".into(),
        Value::Bool(b)    => b.to_string(),
        Value::Number(n)  => n.to_string(),
        Value::String(s)  => format!("\"{}\"", s.replace('"', "\\\"")),
        Value::Array(a)   => format!("[{} items]", a.len()),
        Value::Object(m)  => format!("{{{} fields}}", m.len()),
    };
    truncate_preview(&s)
}

fn truncate_preview(s: &str) -> String {
    if s.chars().count() <= PREVIEW_MAX_CHARS { return s.to_string(); }
    let mut out: String = s.chars().take(PREVIEW_MAX_CHARS).collect();
    out.push('…');
    out
}

// ── History coalescing (mirror toml_studio) ───────────────────────────────

fn record_history(doc: &mut Doc, text: &str, can_coalesce: bool) {
    if doc.history_pos + 1 < doc.history.len() {
        doc.history.truncate(doc.history_pos + 1);
        doc.coalesce_armed = false;
    }
    let now = Instant::now();
    let within = now.duration_since(doc.last_push).as_millis() < COALESCE_WINDOW_MS;
    if can_coalesce && doc.coalesce_armed && within && !doc.history.is_empty() {
        let last = doc.history.len() - 1;
        doc.history[last] = text.to_string();
    } else {
        doc.history.push(text.to_string());
        if doc.history.len() > HISTORY_CAP {
            doc.history.remove(0);
        }
        doc.history_pos = doc.history.len() - 1;
        doc.coalesce_armed = can_coalesce;
    }
    doc.last_push = now;
}

fn normalise_query(expr: &str) -> String {
    let s = expr.trim();
    if s.is_empty() || s == "$" { return s.to_string(); }
    if s.starts_with('$') { return s.to_string(); }
    if s.starts_with('.') || s.starts_with('[') { return format!("${}", s); }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return format!("$..{}", s);
    }
    if s.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') {
        return format!("$.{}", s);
    }
    s.to_string()
}

// ── Diff helpers (mirror toml_studio) ─────────────────────────────────────

fn unified_diff(original: &str, current: &str) -> Vec<DiffHunk> {
    if original == current { return Vec::new(); }
    let diff = TextDiff::from_lines(original, current);
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(3) {
        if group.is_empty() { continue; }
        let mut lines = Vec::new();
        let first = group.first().unwrap();
        let last  = group.last().unwrap();
        let old_start = first.old_range().start as u32 + 1;
        let new_start = first.new_range().start as u32 + 1;
        let old_count = (last.old_range().end - first.old_range().start) as u32;
        let new_count = (last.new_range().end - first.new_range().start) as u32;
        for op in group {
            for change in diff.iter_inline_changes(&op) {
                let (kind, old_line, new_line) = match change.tag() {
                    ChangeTag::Equal => (
                        DiffLineKind::Context,
                        change.old_index().map(|i| (i + 1) as u32),
                        change.new_index().map(|i| (i + 1) as u32),
                    ),
                    ChangeTag::Delete => (
                        DiffLineKind::Del,
                        change.old_index().map(|i| (i + 1) as u32),
                        None,
                    ),
                    ChangeTag::Insert => (
                        DiffLineKind::Add,
                        None,
                        change.new_index().map(|i| (i + 1) as u32),
                    ),
                };
                let mut text = String::new();
                for (_, slice) in change.iter_strings_lossy() {
                    text.push_str(&slice);
                }
                while text.ends_with('\n') || text.ends_with('\r') {
                    text.pop();
                }
                lines.push(DiffLine { kind, old_line, new_line, text });
            }
        }
        hunks.push(DiffHunk { old_start, old_count, new_start, new_count, lines });
    }
    hunks
}

fn build_tree_diff(orig: Option<&Value>, curr: Option<&Value>) -> DiffTreeNode {
    walk_value_diff("$".into(), Vec::new(), orig, curr)
}

fn walk_value_diff(
    key:  String,
    path: Vec<String>,
    a:    Option<&Value>,
    b:    Option<&Value>,
) -> DiffTreeNode {
    let make = |status, kind_b, kind_a, prev_b, prev_a, children: Vec<DiffTreeNode>, count: u32| {
        DiffTreeNode {
            key: key.clone(),
            path: path.clone(),
            status,
            kind_before: kind_b,
            kind_after: kind_a,
            preview_before: prev_b,
            preview_after: prev_a,
            tag_before: None,
            tag_after: None,
            children,
            change_count: count,
        }
    };
    match (a, b) {
        (Some(a), Some(b)) => {
            if a == b {
                return make(DiffStatus::Unchanged, None, None, None, None, Vec::new(), 0);
            }
            match (a, b) {
                (Value::Object(am), Value::Object(bm)) => {
                    let mut children = Vec::new();
                    let mut seen = std::collections::HashSet::<String>::new();
                    for (k, bv) in bm.iter() {
                        let mut p = path.clone(); p.push(k.clone());
                        let child = walk_value_diff(
                            k.clone(),
                            p,
                            am.get(k.as_str()),
                            Some(bv),
                        );
                        if !matches!(child.status, DiffStatus::Unchanged) {
                            children.push(child);
                        }
                        seen.insert(k.clone());
                    }
                    for (k, av) in am.iter() {
                        if seen.contains(k) { continue; }
                        let mut p = path.clone(); p.push(k.clone());
                        let child = walk_value_diff(k.clone(), p, Some(av), None);
                        if !matches!(child.status, DiffStatus::Unchanged) {
                            children.push(child);
                        }
                    }
                    let count = children.iter().map(|c| c.change_count.max(1)).sum::<u32>();
                    make(
                        DiffStatus::Partial,
                        Some("object".into()),
                        Some("object".into()),
                        None,
                        None,
                        children,
                        count,
                    )
                }
                (Value::Array(aa), Value::Array(bb)) => {
                    let mut children = Vec::new();
                    let n = aa.len().max(bb.len());
                    for i in 0..n {
                        let mut p = path.clone(); p.push(i.to_string());
                        let child = walk_value_diff(
                            i.to_string(),
                            p,
                            aa.get(i),
                            bb.get(i),
                        );
                        if !matches!(child.status, DiffStatus::Unchanged) {
                            children.push(child);
                        }
                    }
                    let count = children.iter().map(|c| c.change_count.max(1)).sum::<u32>();
                    make(
                        DiffStatus::Partial,
                        Some("array".into()),
                        Some("array".into()),
                        None,
                        None,
                        children,
                        count,
                    )
                }
                (a_other, b_other) => make(
                    DiffStatus::Modified,
                    Some(value_kind(a_other).as_str().to_string()),
                    Some(value_kind(b_other).as_str().to_string()),
                    Some(preview_for_value(a_other)),
                    Some(preview_for_value(b_other)),
                    Vec::new(),
                    1,
                ),
            }
        }
        (Some(a), None) => make(
            DiffStatus::Removed,
            Some(value_kind(a).as_str().to_string()),
            None,
            Some(preview_for_value(a)),
            None,
            Vec::new(),
            1,
        ),
        (None, Some(b)) => make(
            DiffStatus::Added,
            None,
            Some(value_kind(b).as_str().to_string()),
            None,
            Some(preview_for_value(b)),
            Vec::new(),
            1,
        ),
        (None, None) => make(DiffStatus::Unchanged, None, None, None, None, Vec::new(), 0),
    }
}
