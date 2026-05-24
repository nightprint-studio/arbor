//! TOML Studio — editable TOML document registry (Phase 4.a + 4.b).
//!
//! Owned by `TomlBackend` (see `backend_impl.rs`) which exposes it
//! through the unified `StudioFormatBackend` trait. The doc model:
//!   - `original`  — text the file was opened with, snapshot-immutable.
//!   - `current`   — live edited buffer the FE sees through `raw_current`.
//!   - `doc`       — `toml_edit::DocumentMut` parsed from `current`. The
//!                   key win over JSON's hand-rolled byte-splice machinery
//!                   is that `toml_edit` already preserves comments,
//!                   whitespace, ordering, and inline-vs-table formatting
//!                   natively — mutations re-emit through the document and
//!                   only the touched span changes.
//!   - `value`     — `serde_json::Value` mirror of the document, used for
//!                   children lookup and JSONPath queries (same trick RON
//!                   uses: project the format-native AST to JSON for the
//!                   query engine).
//!   - `history`   — text snapshots backing undo / redo. Typing edits
//!                   coalesce within ~500 ms; structural mutations
//!                   (`apply_mutation`) never coalesce.
//!   - encoding    — sniffed at parse time, round-tripped through save.
//!
//! TOML has no native null; per FROZEN F13 the descriptor declares
//! `null_handling = AsDelete` so the FE bulk-edit modal can warn the user
//! that writing `null` will remove the key. The mutation surface in 4.b
//! doesn't expose null directly — `set_primitive` rejects null, and the
//! delete flow goes through `remove_at`.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json_path::{JsonPath, PathElement};
use similar::{ChangeTag, TextDiff};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value as TomlValue};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::studio::format::types::{
    DiffHunk, DiffLine, DiffLineKind, DiffStatus, DiffTreeNode,
};

pub mod backend_impl;

#[derive(Default)]
pub struct TomlStudioRegistry {
    docs: HashMap<String, Doc>,
}

struct Doc {
    /// Initial text the doc was opened with. Drives the dirty flag +
    /// "diff vs original" view.
    original:       String,
    /// Live edited buffer.
    current:        String,
    /// Parsed `toml_edit::DocumentMut`. None when the buffer is
    /// unparseable — mutations are rejected in that state but the
    /// user can still fix the raw text via `set_text`.
    doc:            Option<DocumentMut>,
    /// `serde_json::Value` projection of `doc`. None when parse failed.
    /// Powers `get_children` / `get_value` / `query` (JSONPath).
    value:          Option<Value>,
    parse_error:    Option<String>,
    /// Indent string the doc was opened with. TOML formatting is
    /// largely owned by `toml_edit` (each table tracks its own
    /// decor), so this is informational — used by the FE's indent
    /// picker for visual feedback only.
    indent:         String,

    source_path:    Option<String>,
    encoding_label: String,
    had_bom:        bool,

    /// Text snapshots backing undo / redo. Always non-empty: the
    /// initial parse pushes the original text as snapshot 0.
    history:        Vec<String>,
    history_pos:    usize,
    coalesce_armed: bool,
    last_push:      Instant,
}

/// Discriminating kind string for the unified
/// `studio::format::types::NodeView.kind` field. The FE's
/// `kind_palette` (see `build_descriptor` in `backend_impl.rs`) maps
/// each variant to a chip style. FROZEN F11: do NOT collapse this to
/// the JSON set — TOML has table-vs-inline-table-vs-array-of-tables
/// distinctions the user cares about.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Top-of-file or `[section]` / `[[arr]]` body — block table.
    Table,
    /// `{ a = 1, b = 2 }` — single-line table value.
    InlineTable,
    /// `[1, 2, 3]` — array of values (NOT array-of-tables).
    Array,
    /// `[[products]] / [[products]] …` — sequence of block tables.
    ArrayOfTables,
    String,
    Integer,
    Float,
    Bool,
    /// TOML datetime literals (offset / local / date / time). We don't
    /// distinguish the four sub-kinds — the preview string carries the
    /// raw text so the FE can tell.
    Datetime,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Table         => "table",
            NodeKind::InlineTable   => "inline_table",
            NodeKind::Array         => "array",
            NodeKind::ArrayOfTables => "array_of_tables",
            NodeKind::String        => "string",
            NodeKind::Integer       => "integer",
            NodeKind::Float         => "float",
            NodeKind::Bool          => "bool",
            NodeKind::Datetime      => "datetime",
        }
    }
}

/// Outcome of `parse`. Mirrors what `backend_impl` lifts into the
/// unified `studio::format::types::ParseResult` for the FE.
#[derive(Debug)]
pub struct ParseResult {
    pub doc_id:      String,
    pub size_bytes:  usize,
    pub root_kind:   Option<NodeKind>,
    pub child_count: usize,
    pub source_path: Option<String>,
    pub parse_error: Option<String>,
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

impl TomlStudioRegistry {
    pub fn parse(
        &mut self,
        text:           String,
        source_path:    Option<String>,
        encoding_label: String,
        had_bom:        bool,
    ) -> ParseResult {
        let size = text.len();
        let (doc, value, parse_error) = parse_pair(&text);
        let kind        = doc.as_ref().map(|d| kind_for_item(d.as_item()));
        let child_count = doc.as_ref().map(|d| child_count_for_item(d.as_item()))
            .unwrap_or(0);
        let indent      = detect_indent(&text);
        let id          = Uuid::new_v4().to_string();
        self.docs.insert(id.clone(), Doc {
            original:       text.clone(),
            current:        text.clone(),
            doc,
            value,
            parse_error:    parse_error.clone(),
            indent,
            source_path:    source_path.clone(),
            encoding_label,
            had_bom,
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
        }
    }

    pub fn close(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    fn doc(&self, doc_id: &str) -> Result<&Doc> {
        self.docs.get(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown TOML Studio doc: {doc_id}")))
    }
    fn doc_mut(&mut self, doc_id: &str) -> Result<&mut Doc> {
        self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown TOML Studio doc: {doc_id}")))
    }

    // ── Tree navigation ────────────────────────────────────────────

    pub fn get_root(&self, doc_id: &str) -> Result<NodeView> {
        let doc = self.doc(doc_id)?;
        let d = doc.doc.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        Ok(view_for_cursor("$".to_string(), Vec::new(), Cursor::Item(d.as_item())))
    }

    pub fn get_children(&self, doc_id: &str, path: &[String]) -> Result<Vec<NodeView>> {
        let doc = self.doc(doc_id)?;
        let d = doc.doc.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        let cur = resolve_cursor(Cursor::Item(d.as_item()), path)
            .ok_or_else(|| AppError::Other(format!("Missing path: {path:?}")))?;
        Ok(children_of_cursor(path, cur))
    }

    pub fn get_value_pretty(&self, doc_id: &str, path: &[String]) -> Result<String> {
        // Project to JSON via the `Value` mirror so the previewer stays
        // consistent with RON / JSON. For a leaf this round-trips
        // through JSON's pretty-printer; for a container the user sees
        // a JSON-flavoured snapshot. TOML round-trip for a single value
        // would require re-serialising the bare value (no `[section]`
        // header) and isn't worth the complexity for the value pane.
        let doc = self.doc(doc_id)?;
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot read value".into()))?;
        let mut cur = v;
        for seg in path {
            cur = match cur {
                Value::Object(map) => map.get(seg)
                    .ok_or_else(|| AppError::Other(format!("Missing key: {seg}")))?,
                Value::Array(arr) => {
                    let i: usize = seg.parse()
                        .map_err(|_| AppError::Other(format!("Invalid array index: {seg}")))?;
                    arr.get(i)
                        .ok_or_else(|| AppError::Other(format!("Array index out of bounds: {i}")))?
                }
                _ => return Err(AppError::Other(format!("Cannot descend into leaf at: {seg}"))),
            };
        }
        serde_json::to_string_pretty(cur).map_err(|e| AppError::Other(e.to_string()))
    }

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
        Ok(self.doc(doc_id)?.doc.as_ref().map(|d| kind_for_item(d.as_item())))
    }
    pub fn root_child_count(&self, doc_id: &str) -> Result<usize> {
        Ok(self.doc(doc_id)?.doc.as_ref()
            .map(|d| child_count_for_item(d.as_item()))
            .unwrap_or(0))
    }
    pub fn get_indent(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.indent.clone())
    }
    pub fn set_indent(&mut self, doc_id: &str, indent: String) -> Result<()> {
        let d = self.doc_mut(doc_id)?;
        d.indent = indent;
        Ok(())
    }
    pub fn history_state(&self, doc_id: &str) -> Result<(bool, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.history_pos > 0, d.history_pos + 1 < d.history.len()))
    }

    /// Pretty-print the document via `toml_edit`'s own serialiser.
    /// Since `toml_edit` natively preserves formatting, "pretty" here
    /// effectively reflows the document — we route it through the
    /// fresh `DocumentMut::to_string()` so any decor anomalies (e.g.
    /// missing trailing newline) normalise.
    pub fn pretty(&self, doc_id: &str) -> Result<String> {
        let doc = self.doc(doc_id)?;
        let d = doc.doc.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot pretty-print".into()))?;
        Ok(d.to_string())
    }

    // ── Editing — text level ───────────────────────────────────────

    pub fn set_text(&mut self, doc_id: &str, text: String) -> Result<UpdateResult> {
        let doc = self.doc_mut(doc_id)?;
        let (parsed_doc, value, parse_error) = parse_pair(&text);
        let root_kind   = parsed_doc.as_ref().map(|d| kind_for_item(d.as_item()));
        let child_count = parsed_doc.as_ref()
            .map(|d| child_count_for_item(d.as_item()))
            .unwrap_or(0);
        record_history(doc, &text, /* can_coalesce */ true);
        doc.current     = text;
        doc.doc         = parsed_doc;
        doc.value       = value;
        doc.parse_error = parse_error.clone();
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
        F: FnOnce(&mut DocumentMut) -> Result<()>,
    {
        let doc = self.doc_mut(doc_id)?;
        let mut working = doc.doc.clone()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot edit tree".into()))?;
        op(&mut working)?;
        let new_text = working.to_string();
        // Re-parse the regenerated text to recover a fresh AST + value
        // mirror. If the mutation produced an invalid document the
        // caller never sees a corrupt registry state.
        let (new_doc, new_value, parse_error) = parse_pair(&new_text);
        if let Some(err) = &parse_error {
            return Err(AppError::Other(format!("Mutation produced invalid TOML: {err}")));
        }
        record_history(doc, &new_text, /* can_coalesce */ false);
        let kind        = new_doc.as_ref().map(|d| kind_for_item(d.as_item()));
        let child_count = new_doc.as_ref()
            .map(|d| child_count_for_item(d.as_item()))
            .unwrap_or(0);
        doc.current     = new_text.clone();
        doc.doc         = new_doc;
        doc.value       = new_value;
        doc.parse_error = None;
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
        self.mutate_with(doc_id, move |doc| {
            let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                .ok_or_else(|| AppError::Other(format!("Path not found: {path:?}")))?;
            let new_val = json_value_to_toml_value(&value)
                .ok_or_else(|| AppError::Other(
                    "Cannot set primitive — value is not a scalar TOML type".into(),
                ))?;
            set_primitive_at(target, new_val)
        })
    }

    pub fn replace_at(
        &mut self,
        doc_id: &str,
        path:   &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        // Parse the snippet as a TOML value (RHS) by wrapping in a
        // throwaway assignment. Lets the user paste a struct, array
        // or inline-table without learning a new mini-grammar.
        let path = path.to_vec();
        self.mutate_with(doc_id, move |doc| {
            let parsed: DocumentMut = format!("__arbor_tmp__ = {snippet}\n")
                .parse()
                .map_err(|e| AppError::Other(format!("Invalid TOML snippet: {e}")))?;
            let new_item = parsed.get("__arbor_tmp__")
                .ok_or_else(|| AppError::Other("Snippet parse: missing value".into()))?
                .clone();
            let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                .ok_or_else(|| AppError::Other(format!("Path not found: {path:?}")))?;
            replace_at_cursor(target, new_item)
        })
    }

    pub fn remove_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot remove document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |doc| {
            let (parent_path, last) = path.split_at(path.len() - 1);
            let parent = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), parent_path)
                .ok_or_else(|| AppError::Other(format!("Parent path not found: {parent_path:?}")))?;
            remove_child_at(parent, &last[0])
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
        self.mutate_with(doc_id, move |doc| {
            let parsed: DocumentMut = format!("__arbor_tmp__ = {snippet}\n")
                .parse()
                .map_err(|e| AppError::Other(format!("Invalid TOML snippet: {e}")))?;
            let new_item = parsed.get("__arbor_tmp__")
                .ok_or_else(|| AppError::Other("Snippet parse: missing value".into()))?
                .clone();
            let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                .ok_or_else(|| AppError::Other(format!("Path not found: {path:?}")))?;
            insert_field_at(target, &name, new_item)
        })
    }

    pub fn insert_item(
        &mut self,
        doc_id:  &str,
        path:    &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        let path = path.to_vec();
        self.mutate_with(doc_id, move |doc| {
            let parsed: DocumentMut = format!("__arbor_tmp__ = {snippet}\n")
                .parse()
                .map_err(|e| AppError::Other(format!("Invalid TOML snippet: {e}")))?;
            let new_item = parsed.get("__arbor_tmp__")
                .ok_or_else(|| AppError::Other("Snippet parse: missing value".into()))?
                .clone();
            let target = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), &path)
                .ok_or_else(|| AppError::Other(format!("Path not found: {path:?}")))?;
            insert_item_at(target, new_item)
        })
    }

    pub fn insert_map_entry(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        key_text: String,
        val_text: String,
    ) -> Result<MutateResult> {
        // TOML maps and tables are the same construct — delegate to
        // `insert_field` semantics. The key is the literal string;
        // `toml_edit` quotes it on serialisation when necessary.
        self.insert_field(doc_id, path, key_text, val_text)
    }

    pub fn duplicate_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        if path.is_empty() {
            return Err(AppError::Other("Cannot duplicate document root".into()));
        }
        let path = path.to_vec();
        self.mutate_with(doc_id, move |doc| {
            // Resolve the source via the immutable cursor and clone
            // its representation into an owned `Item` we can re-
            // insert on the parent without aliasing the doc's borrow.
            let src_cursor = resolve_cursor(Cursor::Item(doc.as_item()), &path)
                .ok_or_else(|| AppError::Other(format!("Path not found: {path:?}")))?;
            let src_item = cursor_to_owned_item(src_cursor);
            let (parent_path, last) = path.split_at(path.len() - 1);
            let parent = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), parent_path)
                .ok_or_else(|| AppError::Other(format!("Parent path not found: {parent_path:?}")))?;
            duplicate_at_cursor(parent, &last[0], src_item)
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
        self.mutate_with(doc_id, move |doc| {
            let (parent_path, last) = path.split_at(path.len() - 1);
            let key = &last[0];
            let parent = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), parent_path)
                .ok_or_else(|| AppError::Other(format!("Parent path not found: {parent_path:?}")))?;
            move_at_cursor(parent, key, delta)
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
        let (parsed, value, parse_error) = parse_pair(&text);
        let kind        = parsed.as_ref().map(|d| kind_for_item(d.as_item()));
        let child_count = parsed.as_ref()
            .map(|d| child_count_for_item(d.as_item()))
            .unwrap_or(0);
        doc.current        = text.clone();
        doc.doc            = parsed;
        doc.value          = value;
        doc.parse_error    = parse_error.clone();
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
        // Project both sides through `serde_json::Value` so the diff
        // walker stays format-agnostic (same trick used elsewhere).
        let orig_val = parse_value_only(&doc.original);
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

    // ── Query (JSON-Path against the Value mirror) ─────────────────

    pub fn query(&self, doc_id: &str, expr: &str) -> Result<Vec<QueryHit>> {
        let doc = self.doc(doc_id)?;
        let root = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot query".into()))?;
        let normalised = normalise_query(expr);
        if normalised.is_empty() { return Ok(Vec::new()); }
        let path = JsonPath::parse(&normalised)
            .map_err(|e| AppError::Other(format!("Query parse error: {e}")))?;
        let located = path.query_located(root);
        let mut hits = Vec::with_capacity(QUERY_MAX_HITS.min(located.len()));
        for ln in located.iter() {
            if hits.len() >= QUERY_MAX_HITS { break; }
            let val: &Value = ln.node();
            let path: Vec<String> = ln.location().iter().map(|el| match el {
                PathElement::Name(s)  => s.to_string(),
                PathElement::Index(i) => i.to_string(),
            }).collect();
            hits.push(QueryHit {
                path,
                kind:    kind_for_value(val),
                preview: preview_for_value(val),
            });
        }
        Ok(hits)
    }
}

// ── F13 — Bulk edit (Phase 4.c.b.1) ─────────────────────────────────────────

/// Concrete value to install at a `set` site (TOML flavour). The
/// preview / apply layers turn an `expr` / `literal` value source into
/// this enum before handing it to `apply_bulk_edits_*`.
#[derive(Debug, Clone)]
pub enum TomlSetValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
}

/// One edit op applied to the TOML buffer.
///
/// Per FROZEN F13 + descriptor `null_handling = AsDelete`, setting a
/// `null` literal on a TOML site converts to `Delete` at the
/// site-builder layer — TOML has no native null. Callers see a unified
/// `Set | Delete` palette either way.
#[derive(Debug, Clone)]
pub enum TomlBulkOp {
    Set(TomlSetValue),
    Delete,
}

/// Apply a batch of `TomlBulkOp`s in place against a mutable
/// `DocumentMut`. Shared by the text-level (`apply_bulk_edits_text`)
/// and doc-level (`apply_bulk_edits_doc`) flows.
///
/// Order:
///   - Phase A (sets) runs first; order among sets is irrelevant.
///   - Phase B (deletes) groups ops by parent path, sorts each
///     parent's keys numeric-aware descending so array-index removes
///     don't shift earlier indices. Parents iterate descending so
///     deeper paths delete before ancestors get a chance to wipe them.
fn apply_bulk_edits_in_place(
    doc: &mut DocumentMut,
    ops: &[(Vec<String>, TomlBulkOp)],
) -> Result<()> {
    // Phase A — sets.
    for (path, op) in ops {
        let TomlBulkOp::Set(val) = op else { continue; };
        let cur = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), path)
            .ok_or_else(|| AppError::Other(format!(
                "Set site path not found: {}",
                path.join("/"),
            )))?;
        let new_val: TomlValue = match val {
            TomlSetValue::String(s)  => TomlValue::from(s.as_str()),
            TomlSetValue::Integer(i) => TomlValue::from(*i),
            TomlSetValue::Float(f)   => TomlValue::from(*f),
            TomlSetValue::Bool(b)    => TomlValue::from(*b),
        };
        set_primitive_at(cur, new_val)?;
    }

    // Phase B — deletes.
    let mut by_parent: std::collections::BTreeMap<Vec<String>, Vec<String>> =
        std::collections::BTreeMap::new();
    for (path, op) in ops {
        if !matches!(op, TomlBulkOp::Delete) { continue; }
        if path.is_empty() {
            return Err(AppError::Other("Cannot delete the document root".into()));
        }
        let (key, parent) = path.split_last().unwrap();
        by_parent.entry(parent.to_vec()).or_default().push(key.clone());
    }
    for (parent_path, mut keys) in by_parent.into_iter().rev() {
        keys.sort_by(|a, b| match (a.parse::<i64>().ok(), b.parse::<i64>().ok()) {
            (Some(ai), Some(bi)) => bi.cmp(&ai),
            _ => b.cmp(a),
        });
        keys.dedup();
        for k in &keys {
            let parent = resolve_cursor_mut(
                CursorMut::Item(doc.as_item_mut()),
                &parent_path,
            )
            .ok_or_else(|| AppError::Other(format!(
                "Parent path not found: {parent_path:?}",
            )))?;
            remove_child_at(parent, k)?;
        }
    }
    Ok(())
}

/// Project-wide flow: parse `input`, run the batch, emit the new
/// document. Pre-flush — caller writes to disk only if this returns Ok.
pub fn apply_bulk_edits_text(
    input: &str,
    ops:   &[(Vec<String>, TomlBulkOp)],
) -> Result<String> {
    let mut doc: DocumentMut = input.parse()
        .map_err(|e| AppError::Other(format!("parse: {e}")))?;
    apply_bulk_edits_in_place(&mut doc, ops)?;
    Ok(doc.to_string())
}

/// Run a JSON-Path expression against the doc's `Value` projection and
/// return owned `(path, value)` pairs. F13 active-doc preview consumer.
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
        let path: Vec<String> = ln.location().iter().map(|el| match el {
            PathElement::Name(s)  => s.to_string(),
            PathElement::Index(i) => i.to_string(),
        }).collect();
        out.push((path, ln.node().clone()));
    }
    Ok(out)
}

/// Per-value kind helper exposed for the bulk-edit preview path so the
/// FE site row carries a stable kind string. Mirrors RON / JSON's
/// `*_kind_str` exports.
pub fn toml_kind_str(v: &Value) -> &'static str {
    kind_for_value(v).as_str()
}

/// Preview helper shared with the bulk-edit site builder so the modal's
/// old-line / new-line look consistent with the tree pane.
pub fn toml_preview_for(v: &Value) -> String { preview_for_value(v) }

impl TomlStudioRegistry {
    /// Run a JSON-Path query against the doc's parsed `Value` and
    /// return owned `(path, value)` pairs. Active-doc F13 entry point.
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

    /// Apply a bulk-edit batch to an open doc. Routes through
    /// `mutate_with` so the result records a single discrete history
    /// entry (one bulk edit = one undo).
    pub fn apply_bulk_edits_doc(
        &mut self,
        doc_id: &str,
        ops:    &[(Vec<String>, TomlBulkOp)],
    ) -> Result<MutateResult> {
        let ops = ops.to_vec();
        self.mutate_with(doc_id, move |doc| apply_bulk_edits_in_place(doc, &ops))
    }
}

// ── Cross-ref helpers (Phase 4.c.a) ─────────────────────────────────────────

/// Parse `text` as a TOML document and project to `serde_json::Value`.
/// Used by `studio::index` + `studio::scan_*` cross-ref walkers to walk
/// TOML alongside RON / JSON without needing a separate TOML traversal
/// surface. Returns `None` on parse error (silently skipped — matches
/// the JSON / RON cross-ref scanners' best-effort policy).
pub fn parse_to_value(text: &str) -> Option<Value> {
    text.parse::<DocumentMut>().ok().map(|d| doc_to_value(&d))
}

/// Splice `new_value` over every TOML string node at the given paths,
/// preserving every byte outside the touched values (`toml_edit`
/// re-emits unchanged decoration verbatim).
///
/// Paths that don't resolve to a string node are reported as an error
/// before any mutation happens (pre-flush validation — matches F12's
/// atomic-by-file contract).
pub fn apply_string_rename(
    text:      &str,
    paths:     &[Vec<String>],
    new_value: &str,
) -> Result<String> {
    let mut doc: DocumentMut = text.parse()
        .map_err(|e| AppError::Other(format!("TOML parse: {e}")))?;

    // Validate every site first so a failure on any path aborts before
    // we mutate anything.
    for path in paths {
        let cur = resolve_cursor(Cursor::Item(doc.as_item()), path)
            .ok_or_else(|| AppError::Other(format!(
                "Rename site path not found: {}", path.join("/"),
            )))?;
        let is_string = match cur {
            Cursor::Item(Item::Value(TomlValue::String(_))) => true,
            Cursor::Value(TomlValue::String(_))             => true,
            _ => false,
        };
        if !is_string {
            return Err(AppError::Other(format!(
                "Rename site at {path:?} is not a string leaf",
            )));
        }
    }

    // Apply.
    for path in paths {
        let cur = resolve_cursor_mut(CursorMut::Item(doc.as_item_mut()), path)
            .ok_or_else(|| AppError::Other(format!(
                "Rename site path vanished mid-apply: {}", path.join("/"),
            )))?;
        write_string_at(cur, new_value);
    }
    Ok(doc.to_string())
}

/// Set a TOML string at the cursor, preserving the existing value's
/// decor (surrounding whitespace / quote style hints).
fn write_string_at(target: CursorMut<'_>, new_value: &str) {
    match target {
        CursorMut::Item(item) => {
            let decor = if let Item::Value(v) = item {
                Some(v.decor().clone())
            } else {
                None
            };
            let mut nv = TomlValue::from(new_value);
            if let Some(d) = decor { *nv.decor_mut() = d; }
            *item = Item::Value(nv);
        }
        CursorMut::Value(v) => {
            let decor = v.decor().clone();
            let mut nv = TomlValue::from(new_value);
            *nv.decor_mut() = decor;
            *v = nv;
        }
        CursorMut::Table(_) => {
            // Validated as a string before entering apply phase —
            // unreachable in practice.
        }
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

// ── Parse helpers ───────────────────────────────────────────────────────────

/// Parse `text` as a `toml_edit::DocumentMut` AND project to a
/// `serde_json::Value` mirror. The two views coexist: `DocumentMut` is
/// the source-of-truth for edits + formatting; `Value` is the source-of-
/// truth for navigation + queries.
fn parse_pair(text: &str) -> (Option<DocumentMut>, Option<Value>, Option<String>) {
    match text.parse::<DocumentMut>() {
        Ok(doc) => {
            let value = doc_to_value(&doc);
            (Some(doc), Some(value), None)
        }
        Err(e) => (None, None, Some(format!("TOML parse error: {e}"))),
    }
}

/// Parse without keeping the AST — used by `tree_diff` to grab the
/// `Value` mirror for the original buffer without paying for a full
/// `DocumentMut` clone.
fn parse_value_only(text: &str) -> Option<Value> {
    text.parse::<DocumentMut>().ok().map(|d| doc_to_value(&d))
}

fn doc_to_value(doc: &DocumentMut) -> Value {
    item_to_value(doc.as_item())
}

fn item_to_value(item: &Item) -> Value {
    match item {
        Item::None        => Value::Null,
        Item::Value(v)    => toml_value_to_json(v),
        Item::Table(t)    => {
            let mut map = serde_json::Map::new();
            for (k, v) in t.iter() {
                map.insert(k.to_string(), item_to_value(v));
            }
            Value::Object(map)
        }
        Item::ArrayOfTables(arr) => {
            let mut items = Vec::with_capacity(arr.len());
            for t in arr.iter() {
                let mut map = serde_json::Map::new();
                for (k, v) in t.iter() {
                    map.insert(k.to_string(), item_to_value(v));
                }
                items.push(Value::Object(map));
            }
            Value::Array(items)
        }
    }
}

fn toml_value_to_json(v: &TomlValue) -> Value {
    match v {
        TomlValue::String(s) => Value::String(s.value().clone()),
        TomlValue::Integer(i) => Value::Number((*i.value()).into()),
        TomlValue::Float(f) => {
            serde_json::Number::from_f64(*f.value())
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        TomlValue::Boolean(b) => Value::Bool(*b.value()),
        // Datetimes serialise to their textual form. The user-visible
        // preview shows the raw token; query / get_value treat the
        // datetime as a string for projection purposes.
        TomlValue::Datetime(d) => Value::String(d.value().to_string()),
        TomlValue::Array(a) => Value::Array(a.iter().map(toml_value_to_json).collect()),
        TomlValue::InlineTable(t) => {
            let mut map = serde_json::Map::new();
            for (k, v) in t.iter() {
                map.insert(k.to_string(), toml_value_to_json(v));
            }
            Value::Object(map)
        }
    }
}

// ── Kind / preview helpers ──────────────────────────────────────────────────

fn kind_for_item(item: &Item) -> NodeKind {
    match item {
        Item::Table(_)         => NodeKind::Table,
        Item::ArrayOfTables(_) => NodeKind::ArrayOfTables,
        Item::Value(v)         => kind_for_value_toml(v),
        // `Item::None` shouldn't normally surface in the tree (we
        // never store one as a direct child), but if it does we treat
        // it as an empty placeholder — call it a Table for lack of a
        // better option.
        Item::None             => NodeKind::Table,
    }
}

fn kind_for_value_toml(v: &TomlValue) -> NodeKind {
    match v {
        TomlValue::String(_)      => NodeKind::String,
        TomlValue::Integer(_)     => NodeKind::Integer,
        TomlValue::Float(_)       => NodeKind::Float,
        TomlValue::Boolean(_)     => NodeKind::Bool,
        TomlValue::Datetime(_)    => NodeKind::Datetime,
        TomlValue::Array(_)       => NodeKind::Array,
        TomlValue::InlineTable(_) => NodeKind::InlineTable,
    }
}

fn kind_for_value(v: &Value) -> NodeKind {
    // Used by query hits where the source-of-truth is the JSON mirror
    // (no `Item` available). We can't tell Datetime / Array-of-Tables
    // back from the projection — fall through to the closest JSON
    // analogue. Pragmatic loss for query hits; the tree pane (which
    // does have the `Item`) keeps the precise kind.
    match v {
        Value::Null      => NodeKind::String,
        Value::Bool(_)   => NodeKind::Bool,
        Value::Number(n) => if n.is_f64() && !n.is_i64() && !n.is_u64() {
            NodeKind::Float
        } else {
            NodeKind::Integer
        },
        Value::String(_) => NodeKind::String,
        Value::Array(_)  => NodeKind::Array,
        Value::Object(_) => NodeKind::InlineTable,
    }
}

fn child_count_for_item(item: &Item) -> usize {
    match item {
        Item::Table(t)         => t.iter().count(),
        Item::ArrayOfTables(a) => a.len(),
        Item::Value(v) => match v {
            TomlValue::Array(a)       => a.len(),
            TomlValue::InlineTable(t) => t.iter().count(),
            _ => 0,
        },
        Item::None => 0,
    }
}

fn preview_for_item(item: &Item) -> String {
    match item {
        Item::Table(t) => {
            let n = t.iter().count();
            format!("{{{n} {}}}", if n == 1 { "key" } else { "keys" })
        }
        Item::ArrayOfTables(a) => {
            let n = a.len();
            format!("[[{n} {}]]", if n == 1 { "table" } else { "tables" })
        }
        Item::Value(v) => preview_for_value_toml(v),
        Item::None     => String::new(),
    }
}

fn preview_for_value_toml(v: &TomlValue) -> String {
    match v {
        TomlValue::String(s) => {
            let s = s.value();
            let mut out = String::with_capacity(s.len().min(PREVIEW_MAX_CHARS) + 2);
            out.push('"');
            for (i, ch) in s.chars().enumerate() {
                if i >= PREVIEW_MAX_CHARS { out.push('…'); break; }
                out.push(ch);
            }
            out.push('"');
            out
        }
        TomlValue::Integer(i)     => i.value().to_string(),
        TomlValue::Float(f)       => f.value().to_string(),
        TomlValue::Boolean(b)     => b.value().to_string(),
        TomlValue::Datetime(d)    => d.value().to_string(),
        TomlValue::Array(a)       => {
            let n = a.len();
            format!("[{n} {}]", if n == 1 { "item" } else { "items" })
        }
        TomlValue::InlineTable(t) => {
            let n = t.iter().count();
            format!("{{{n} {}}}", if n == 1 { "key" } else { "keys" })
        }
    }
}

fn preview_for_value(v: &Value) -> String {
    match v {
        Value::Object(m) => format!("{{{} keys}}", m.len()),
        Value::Array(a)  => format!("[{} items]", a.len()),
        Value::String(s) => {
            let mut out = String::with_capacity(s.len().min(PREVIEW_MAX_CHARS) + 2);
            out.push('"');
            for (i, ch) in s.chars().enumerate() {
                if i >= PREVIEW_MAX_CHARS { out.push('…'); break; }
                out.push(ch);
            }
            out.push('"');
            out
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b)   => b.to_string(),
        Value::Null      => "null".to_string(),
    }
}

// ── Cursor — navigates Items + nested Values + tables-in-arrays-of-tables. ──

/// Borrowed view into a TOML AST node. We need three variants because
/// `Item::Table` and `Item::ArrayOfTables(_)` entries each contain an
/// owned `Table` which we can't repackage as `&Item` without storage.
/// Same for the leaves of `Value::Array` / `InlineTable` — they're
/// `Value`s, not `Item`s. Carrying the discriminator inline keeps the
/// walker borrow-clean.
#[derive(Clone, Copy)]
enum Cursor<'a> {
    Item(&'a Item),
    Table(&'a Table),
    Value(&'a TomlValue),
}

fn resolve_cursor<'a>(start: Cursor<'a>, path: &[String]) -> Option<Cursor<'a>> {
    let mut cur = start;
    for seg in path {
        cur = step_cursor(cur, seg)?;
    }
    Some(cur)
}

fn step_cursor<'a>(c: Cursor<'a>, seg: &str) -> Option<Cursor<'a>> {
    match c {
        Cursor::Item(Item::Table(t)) => step_table(t, seg),
        Cursor::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = seg.parse().ok()?;
            arr.get(i).map(Cursor::Table)
        }
        Cursor::Item(Item::Value(v)) => step_value(v, seg),
        Cursor::Item(Item::None) => None,
        Cursor::Table(t) => step_table(t, seg),
        Cursor::Value(v) => step_value(v, seg),
    }
}

fn step_table<'a>(t: &'a Table, seg: &str) -> Option<Cursor<'a>> {
    t.get(seg).map(Cursor::Item)
}

fn step_value<'a>(v: &'a TomlValue, seg: &str) -> Option<Cursor<'a>> {
    match v {
        TomlValue::Array(arr) => {
            let i: usize = seg.parse().ok()?;
            arr.get(i).map(Cursor::Value)
        }
        TomlValue::InlineTable(t) => t.get(seg).map(Cursor::Value),
        _ => None,
    }
}

fn kind_for_cursor(c: Cursor<'_>) -> NodeKind {
    match c {
        Cursor::Item(item)  => kind_for_item(item),
        Cursor::Table(_)    => NodeKind::Table,
        Cursor::Value(v)    => kind_for_value_toml(v),
    }
}

fn preview_for_cursor(c: Cursor<'_>) -> String {
    match c {
        Cursor::Item(item) => preview_for_item(item),
        Cursor::Table(t)   => {
            let n = t.iter().count();
            format!("{{{n} {}}}", if n == 1 { "key" } else { "keys" })
        }
        Cursor::Value(v) => preview_for_value_toml(v),
    }
}

fn child_count_for_cursor(c: Cursor<'_>) -> usize {
    match c {
        Cursor::Item(item) => child_count_for_item(item),
        Cursor::Table(t)   => t.iter().count(),
        Cursor::Value(v) => match v {
            TomlValue::Array(a)       => a.len(),
            TomlValue::InlineTable(t) => t.iter().count(),
            _ => 0,
        },
    }
}

fn view_for_cursor(key: String, path: Vec<String>, c: Cursor<'_>) -> NodeView {
    NodeView {
        key,
        path,
        kind:        kind_for_cursor(c),
        preview:     preview_for_cursor(c),
        child_count: child_count_for_cursor(c),
    }
}

fn children_of_cursor(parent_path: &[String], c: Cursor<'_>) -> Vec<NodeView> {
    match c {
        Cursor::Item(Item::Table(t)) | Cursor::Table(t) => {
            t.iter().map(|(k, v)| {
                let mut p = parent_path.to_vec();
                p.push(k.to_string());
                view_for_cursor(k.to_string(), p, Cursor::Item(v))
            }).collect()
        }
        Cursor::Item(Item::ArrayOfTables(arr)) => {
            arr.iter().enumerate().map(|(i, t)| {
                let key = i.to_string();
                let mut p = parent_path.to_vec();
                p.push(key.clone());
                view_for_cursor(key, p, Cursor::Table(t))
            }).collect()
        }
        Cursor::Item(Item::Value(v)) | Cursor::Value(v) => match v {
            TomlValue::Array(arr) => {
                arr.iter().enumerate().map(|(i, vv)| {
                    let key = i.to_string();
                    let mut p = parent_path.to_vec();
                    p.push(key.clone());
                    view_for_cursor(key, p, Cursor::Value(vv))
                }).collect()
            }
            TomlValue::InlineTable(t) => {
                t.iter().map(|(k, vv)| {
                    let key = k.to_string();
                    let mut p = parent_path.to_vec();
                    p.push(key.clone());
                    view_for_cursor(key, p, Cursor::Value(vv))
                }).collect()
            }
            _ => Vec::new(),
        },
        Cursor::Item(Item::None) => Vec::new(),
    }
}

// ── Mutable cursor — navigates Items + nested Values + AoT tables. ─────────

/// Mutable variant of `Cursor`. Mutations need an interior mutable
/// borrow to the leaf; the dispatch helpers below match on the
/// `CursorMut` variant and call the right `toml_edit` setter.
enum CursorMut<'a> {
    Item(&'a mut Item),
    Table(&'a mut Table),
    Value(&'a mut TomlValue),
}

fn resolve_cursor_mut<'a>(start: CursorMut<'a>, path: &[String]) -> Option<CursorMut<'a>> {
    let mut cur = start;
    for seg in path {
        cur = step_cursor_mut(cur, seg)?;
    }
    Some(cur)
}

fn step_cursor_mut<'a>(c: CursorMut<'a>, seg: &str) -> Option<CursorMut<'a>> {
    match c {
        CursorMut::Item(item) => match item {
            Item::Table(t)         => t.get_mut(seg).map(CursorMut::Item),
            Item::ArrayOfTables(a) => {
                let i: usize = seg.parse().ok()?;
                a.get_mut(i).map(CursorMut::Table)
            }
            Item::Value(v) => step_value_mut(v, seg),
            Item::None     => None,
        },
        CursorMut::Table(t) => t.get_mut(seg).map(CursorMut::Item),
        CursorMut::Value(v) => step_value_mut(v, seg),
    }
}

fn step_value_mut<'a>(v: &'a mut TomlValue, seg: &str) -> Option<CursorMut<'a>> {
    match v {
        TomlValue::Array(arr) => {
            let i: usize = seg.parse().ok()?;
            arr.get_mut(i).map(CursorMut::Value)
        }
        TomlValue::InlineTable(t) => t.get_mut(seg).map(CursorMut::Value),
        _ => None,
    }
}

// ── Mutation helpers ────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` into a `toml_edit::Value`. Returns
/// `None` for nulls and arrays — `set_primitive` is for scalar leaves
/// only; structural replacements go through `replace_at`.
fn json_value_to_toml_value(v: &Value) -> Option<TomlValue> {
    // The FE wire format may be tagged (`{type, value}`) or raw — unwrap
    // the tagged form so the match below sees the actual scalar. Mirror
    // of yaml_studio / json_studio / properties_studio.
    let unwrapped: Value;
    let v = if let Value::Object(map) = v {
        if map.len() == 2 && map.contains_key("type") && map.contains_key("value") {
            unwrapped = map.get("value").cloned().unwrap_or(Value::Null);
            &unwrapped
        } else { v }
    } else { v };
    match v {
        Value::Bool(b)   => Some(TomlValue::from(*b)),
        Value::String(s) => Some(TomlValue::from(s.as_str())),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(TomlValue::from(i))
            } else if let Some(f) = n.as_f64() {
                Some(TomlValue::from(f))
            } else {
                None
            }
        }
        // TOML has no null. `null_handling = AsDelete` lives at the
        // descriptor level; mutating a scalar to null is rejected here
        // so the caller is forced to use `remove_at` explicitly.
        Value::Null      => None,
        Value::Array(_) | Value::Object(_) => None,
    }
}

/// Set a scalar primitive at the cursor, preserving surrounding decor
/// where possible.
fn set_primitive_at(target: CursorMut<'_>, new_val: TomlValue) -> Result<()> {
    match target {
        CursorMut::Item(item) => {
            let decor = match item {
                Item::Value(v) => Some(v.decor().clone()),
                _ => None,
            };
            let mut nv = new_val;
            if let Some(d) = decor { *nv.decor_mut() = d; }
            *item = Item::Value(nv);
            Ok(())
        }
        CursorMut::Value(v) => {
            let decor = v.decor().clone();
            let mut nv = new_val;
            *nv.decor_mut() = decor;
            *v = nv;
            Ok(())
        }
        CursorMut::Table(_) => Err(AppError::Other(
            "Cannot set a primitive on a table node".into(),
        )),
    }
}

/// Replace the entire item at the cursor with an arbitrary `Item`
/// (used by `replace_at` / `paste over`).
fn replace_at_cursor(target: CursorMut<'_>, new_item: Item) -> Result<()> {
    match target {
        CursorMut::Item(item) => {
            *item = new_item;
            Ok(())
        }
        CursorMut::Value(v) => {
            let nv = item_to_inline_value(new_item)?;
            *v = nv;
            Ok(())
        }
        CursorMut::Table(_) => Err(AppError::Other(
            "Cannot replace an array-of-tables entry as a whole — descend into a field instead".into(),
        )),
    }
}

/// Convert an `Item` into a `Value` for placement inside an inline
/// table or value array. Block tables are demoted to inline.
fn item_to_inline_value(item: Item) -> Result<TomlValue> {
    match item {
        Item::Value(v) => Ok(v),
        Item::Table(t) => {
            let mut inline = InlineTable::new();
            for (k, v) in t.iter() {
                if let Item::Value(val) = v {
                    inline.insert(k, val.clone());
                }
            }
            Ok(TomlValue::InlineTable(inline))
        }
        Item::ArrayOfTables(_) => Err(AppError::Other(
            "Cannot place an array-of-tables inside a value container".into(),
        )),
        Item::None => Err(AppError::Other("Empty item — nothing to place".into())),
    }
}

fn insert_field_at(target: CursorMut<'_>, key: &str, value: Item) -> Result<()> {
    match target {
        CursorMut::Item(Item::Table(t)) | CursorMut::Table(t) => {
            if t.contains_key(key) {
                return Err(AppError::Other(format!("Key already exists: {key}")));
            }
            t.insert(key, value);
            Ok(())
        }
        CursorMut::Item(Item::Value(TomlValue::InlineTable(t))) | CursorMut::Value(TomlValue::InlineTable(t)) => {
            if t.contains_key(key) {
                return Err(AppError::Other(format!("Key already exists: {key}")));
            }
            let v = item_to_inline_value(value)?;
            t.insert(key, v);
            Ok(())
        }
        _ => Err(AppError::Other(
            "Cannot add a field — target is not a table or inline table".into(),
        )),
    }
}

fn insert_item_at(target: CursorMut<'_>, value: Item) -> Result<()> {
    match target {
        CursorMut::Item(Item::Value(TomlValue::Array(arr))) | CursorMut::Value(TomlValue::Array(arr)) => {
            let v = item_to_inline_value(value)?;
            arr.push(v);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let tbl = match value {
                Item::Table(t) => t,
                Item::Value(TomlValue::InlineTable(t)) => {
                    let mut block = Table::new();
                    for (k, v) in t.iter() {
                        block.insert(k, Item::Value(v.clone()));
                    }
                    block
                }
                _ => return Err(AppError::Other(
                    "Cannot push a non-table into an array-of-tables".into(),
                )),
            };
            arr.push(tbl);
            Ok(())
        }
        _ => Err(AppError::Other(
            "Cannot add an item — target is not an array".into(),
        )),
    }
}

fn remove_child_at(parent: CursorMut<'_>, key: &str) -> Result<()> {
    match parent {
        CursorMut::Item(Item::Table(t)) | CursorMut::Table(t) => {
            t.remove(key)
                .ok_or_else(|| AppError::Other(format!("Key not found: {key}")))
                .map(|_| ())
        }
        CursorMut::Item(Item::Value(TomlValue::InlineTable(t))) | CursorMut::Value(TomlValue::InlineTable(t)) => {
            t.remove(key)
                .ok_or_else(|| AppError::Other(format!("Key not found: {key}")))
                .map(|_| ())
        }
        CursorMut::Item(Item::Value(TomlValue::Array(arr))) | CursorMut::Value(TomlValue::Array(arr)) => {
            let i: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(AppError::Other(format!("Array index out of bounds: {i}")));
            }
            arr.remove(i);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(AppError::Other(format!("Array index out of bounds: {i}")));
            }
            arr.remove(i);
            Ok(())
        }
        _ => Err(AppError::Other("Parent is not a container".into())),
    }
}

fn duplicate_at_cursor(parent: CursorMut<'_>, key: &str, source: Item) -> Result<()> {
    match parent {
        CursorMut::Item(Item::Table(t)) | CursorMut::Table(t) => {
            let mut next_key = format!("{key}_copy");
            let mut n = 2;
            while t.contains_key(&next_key) {
                next_key = format!("{key}_copy{n}");
                n += 1;
            }
            t.insert(&next_key, source);
            Ok(())
        }
        CursorMut::Item(Item::Value(TomlValue::InlineTable(t))) | CursorMut::Value(TomlValue::InlineTable(t)) => {
            let mut next_key = format!("{key}_copy");
            let mut n = 2;
            while t.contains_key(&next_key) {
                next_key = format!("{key}_copy{n}");
                n += 1;
            }
            let v = item_to_inline_value(source)?;
            t.insert(&next_key, v);
            Ok(())
        }
        CursorMut::Item(Item::Value(TomlValue::Array(arr))) | CursorMut::Value(TomlValue::Array(arr)) => {
            let i: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(AppError::Other(format!("Array index out of bounds: {i}")));
            }
            let v = item_to_inline_value(source)?;
            arr.insert(i + 1, v);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {key}")))?;
            if i >= arr.len() {
                return Err(AppError::Other(format!("Array index out of bounds: {i}")));
            }
            let tbl = match source {
                Item::Table(t) => t,
                _ => return Err(AppError::Other(
                    "Cannot duplicate non-table entry in array-of-tables".into(),
                )),
            };
            // `ArrayOfTables` exposes no `insert(usize, Table)` and
            // `remove` returns `()` in toml_edit 0.22 — clone the
            // contents, clear in place, splice in the duplicate, then
            // push everything back. Length is typically small enough
            // that this is cheap.
            let mut all: Vec<Table> = arr.iter().cloned().collect();
            while !arr.is_empty() { arr.remove(0); }
            all.insert(i + 1, tbl);
            for t in all { arr.push(t); }
            Ok(())
        }
        _ => Err(AppError::Other("Parent is not a container".into())),
    }
}

fn move_at_cursor(parent: CursorMut<'_>, key: &str, delta: i32) -> Result<()> {
    match parent {
        CursorMut::Item(Item::Value(TomlValue::Array(arr))) | CursorMut::Value(TomlValue::Array(arr)) => {
            let i: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {key}")))?;
            let new_i = (i as i32 + delta).max(0) as usize;
            let new_i = new_i.min(arr.len().saturating_sub(1));
            if new_i == i { return Ok(()); }
            let item = arr.remove(i);
            arr.insert(new_i, item);
            Ok(())
        }
        CursorMut::Item(Item::ArrayOfTables(arr)) => {
            let i: usize = key.parse()
                .map_err(|_| AppError::Other(format!("Invalid array index: {key}")))?;
            let new_i = (i as i32 + delta).max(0) as usize;
            let new_i = new_i.min(arr.len().saturating_sub(1));
            if new_i == i { return Ok(()); }
            let mut all: Vec<Table> = arr.iter().cloned().collect();
            while !arr.is_empty() { arr.remove(0); }
            let item = all.remove(i);
            all.insert(new_i, item);
            for t in all { arr.push(t); }
            Ok(())
        }
        _ => Err(AppError::Other(
            "Cannot move — parent is not an ordered container".into(),
        )),
    }
}

/// Clone the AST node a `Cursor` points at into an owned `Item` for
/// re-insertion (used by `duplicate_at`).
fn cursor_to_owned_item(c: Cursor<'_>) -> Item {
    match c {
        Cursor::Item(item) => item.clone(),
        Cursor::Table(t)   => Item::Table(t.clone()),
        Cursor::Value(v)   => Item::Value(v.clone()),
    }
}

// ── Misc helpers ────────────────────────────────────────────────────────────

fn detect_indent(text: &str) -> String {
    // Find the first indented line under a table / array. Falls back
    // to two spaces. Most hand-written TOML uses no indent at all
    // (TOML's flat structure makes indent decorative); we still
    // surface a value so the FE's "indent" footer pill has something
    // meaningful to render.
    for line in text.lines() {
        let leading: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        if !leading.is_empty() {
            return leading;
        }
    }
    "  ".into()
}

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

// ── Diff helpers ────────────────────────────────────────────────────────────

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
    match (a, b) {
        (Some(a), Some(b)) => {
            if a == b {
                return DiffTreeNode {
                    key, path,
                    status:         DiffStatus::Unchanged,
                    kind_before:    None,
                    kind_after:     None,
                    preview_before: None,
                    preview_after:  None,
                    tag_before:     None,
                    tag_after:      None,
                    children:       Vec::new(),
                    change_count:   0,
                };
            }
            match (a, b) {
                (Value::Object(am), Value::Object(bm)) => {
                    let mut children = Vec::new();
                    let mut seen = std::collections::HashSet::<String>::new();
                    for (k, bv) in bm.iter() {
                        let av = am.get(k);
                        let mut p = path.clone(); p.push(k.clone());
                        let node = walk_value_diff(k.clone(), p, av, Some(bv));
                        if node.status != DiffStatus::Unchanged { children.push(node); }
                        seen.insert(k.clone());
                    }
                    for (k, av) in am.iter() {
                        if seen.contains(k) { continue; }
                        let mut p = path.clone(); p.push(k.clone());
                        children.push(walk_value_diff(k.clone(), p, Some(av), None));
                    }
                    let cc: u32 = children.iter().map(|c| c.change_count).sum();
                    let status = if cc == 0 { DiffStatus::Unchanged } else { DiffStatus::Partial };
                    DiffTreeNode {
                        key, path, status,
                        kind_before:    Some("table".into()),
                        kind_after:     Some("table".into()),
                        preview_before: Some(format!("{{{} keys}}", am.len())),
                        preview_after:  Some(format!("{{{} keys}}", bm.len())),
                        tag_before:     None,
                        tag_after:      None,
                        children,
                        change_count:   cc,
                    }
                }
                (Value::Array(aa), Value::Array(ba)) => {
                    let mut children = Vec::new();
                    let max = aa.len().max(ba.len());
                    for i in 0..max {
                        let ai = aa.get(i);
                        let bi = ba.get(i);
                        let mut p = path.clone(); p.push(i.to_string());
                        let node = walk_value_diff(i.to_string(), p, ai, bi);
                        if node.status != DiffStatus::Unchanged { children.push(node); }
                    }
                    let cc: u32 = children.iter().map(|c| c.change_count).sum();
                    let status = if cc == 0 { DiffStatus::Unchanged } else { DiffStatus::Partial };
                    DiffTreeNode {
                        key, path, status,
                        kind_before:    Some("array".into()),
                        kind_after:     Some("array".into()),
                        preview_before: Some(format!("[{} items]", aa.len())),
                        preview_after:  Some(format!("[{} items]", ba.len())),
                        tag_before:     None,
                        tag_after:      None,
                        children,
                        change_count:   cc,
                    }
                }
                _ => DiffTreeNode {
                    key, path,
                    status:         DiffStatus::Modified,
                    kind_before:    Some(kind_for_value(a).as_str().to_string()),
                    kind_after:     Some(kind_for_value(b).as_str().to_string()),
                    preview_before: Some(preview_for_value(a)),
                    preview_after:  Some(preview_for_value(b)),
                    tag_before:     None,
                    tag_after:      None,
                    children:       Vec::new(),
                    change_count:   1,
                },
            }
        }
        (Some(a), None) => DiffTreeNode {
            key, path,
            status:         DiffStatus::Removed,
            kind_before:    Some(kind_for_value(a).as_str().to_string()),
            kind_after:     None,
            preview_before: Some(preview_for_value(a)),
            preview_after:  None,
            tag_before:     None,
            tag_after:      None,
            children:       Vec::new(),
            change_count:   1,
        },
        (None, Some(b)) => DiffTreeNode {
            key, path,
            status:         DiffStatus::Added,
            kind_before:    None,
            kind_after:     Some(kind_for_value(b).as_str().to_string()),
            preview_before: None,
            preview_after:  Some(preview_for_value(b)),
            tag_before:     None,
            tag_after:      None,
            children:       Vec::new(),
            change_count:   1,
        },
        (None, None) => DiffTreeNode {
            key, path,
            status:         DiffStatus::Unchanged,
            kind_before:    None,
            kind_after:     None,
            preview_before: None,
            preview_after:  None,
            tag_before:     None,
            tag_after:      None,
            children:       Vec::new(),
            change_count:   0,
        },
    }
}
