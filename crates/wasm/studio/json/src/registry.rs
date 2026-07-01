//! JSON Studio — editable JSON document registry (since Phase 3.b).
//!
//! Owned by `JsonBackend` (see `backend_impl.rs`) which exposes it
//! through the unified `StudioFormatBackend` trait. The doc model:
//! - `original` — text the file was opened with, snapshot-immutable.
//! - `current` — live edited buffer the FE sees through `raw_current`.
//! - `ast` — `jsonc-parser`-derived owned tree with byte ranges,
//!   used by every mutation (path resolution + byte
//!   splice). Refreshed after every successful edit.
//! - `history` — text snapshots backing undo / redo. Typing edits
//!   coalesce within ~500 ms; structural mutations
//!   (`apply_mutation`) never coalesce.
//! - encoding — sniffed at parse time, round-tripped through save.
//!
//! Mutations are *position-preserving*: editing a value at line 500
//! splices bytes at line 500 only — everything else stays byte-for-byte
//! identical. That's the descriptor's `supports_lossless_edit = true`
//! contract. See `edits.rs` for the splice machinery.
//!
//! Read-only navigation (get_root / get_children / get_value / query)
//! still goes through `simd-json` → `serde_json::Value` for the heavier
//! JSON-Path engine and the lazy children tree. The two representations
//! coexist: `ast` is the source-of-truth for edits + ranges, `value` is
//! the source-of-truth for navigation + queries. Both are rebuilt from
//! `current` after every mutation.

use std::collections::HashMap;

use arbor_studio_core::prelude::History;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::err::{AppError, Result};
use arbor_studio_types::prelude::{DiffHunk, DiffStatus, DiffTreeNode};

use crate::{ast, bulk_edits, edits};

#[derive(Default)]
pub struct JsonStudioRegistry {
    docs: HashMap<String, Doc>,
}

/// Per-doc parsing strategy. Phase 3.d:
///
/// - `Tree` — `jsonc-parser` AST + byte ranges. Full editing surface
///   (mutations, undo/redo, JSONC features when the file is `.jsonc` or
///   the buffer happens to contain comments / trailing commas in a
///   `.json` document). Used for files under `stream_threshold_bytes`.
///
/// - `Stream` — `simd_json` strict only, NO AST. Navigation + queries
///   work via `value`; structural mutations (`apply_mutation`) return
///   `Unsupported`. CodeMirror typing in the text pane still works
///   (re-parses on every `set_text`) but bulk byte-splice editing is
///   disabled. The mode is sticky for the lifetime of the doc — flipping
///   mid-session would surprise the user (suddenly editable / suddenly
///   not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocParseMode {
    Tree,
    Stream,
}

struct Doc {
    /// Initial text the doc was opened with. Stays put across edits and
    /// drives the "diff vs original" view + the dirty flag (current != original).
    original:    String,
    /// Live edited buffer — what the modal renders + what `raw_current`
    /// returns.
    current:     String,
    /// Pretty-printed text of the document, lazily produced and cached
    /// so flipping back and forth between Raw and Pretty doesn't
    /// re-serialise. Invalidated on every edit.
    pretty:      Option<String>,
    /// Parsed AST view of `current` (None when parse fails or the doc
    /// is in `Stream` mode — large files skip AST construction
    /// entirely). Mutations resolve paths against this and splice bytes
    /// inside `current`.
    ast:         Option<ast::JsonAst>,
    /// `Value` view of `current` (None when parse fails). Powers
    /// children-of, JSON-Path queries and the deep get-value lookup.
    /// Lazily rebuilt to avoid double parsing when callers only need
    /// the AST.
    value:       Option<Value>,
    /// `None` when parse succeeded. `Some(msg)` when the buffer is
    /// invalid — the modal still gets a `doc_id` so the user can fix
    /// the text + retry.
    parse_error: Option<String>,
    /// Indent string used by `format_doc` for the pretty view. Detected
    /// from the source on parse, two-space default otherwise. The
    /// mutation splicer uses its own indent probe per-container — this
    /// is for the bulk pretty rendering only.
    indent:      String,

    /// Sticky per-doc parsing strategy (Phase 3.d). See `DocParseMode`.
    parse_mode:  DocParseMode,
    /// `true` when the doc was opened from a `.jsonc` extension. Drives
    /// the FE banners: `.json + has_jsonc_features` shows the "rename
    /// to .jsonc / strip" prompt; `.jsonc + has_jsonc_features` does
    /// not (expected behaviour).
    is_jsonc:    bool,
    /// `true` when the current buffer contains JSONC-only constructs
    /// (comments or trailing commas). Recomputed on every successful
    /// `set_text` / mutation so the flag tracks the live buffer, not
    /// the original. Always `false` in `Stream` mode (strict parser).
    has_jsonc_features: bool,

    source_path:    Option<String>,
    encoding_label: String,
    had_bom:        bool,

    /// Text snapshots backing undo / redo. Engine lives in
    /// `arbor_studio_core::prelude::history::History`. Typing edits
    /// coalesce within ~500ms (`record_text`); structured mutations never
    /// coalesce (`record_struct`). Cap 200, no no-op suppression.
    history:     History<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
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

    // Phase 3.d ────────────────────────────────────────────────────────
    pub parse_mode:          DocParseMode,
    pub is_jsonc:            bool,
    pub has_jsonc_features:  bool,
}

/// Outcome of a non-structured text edit — the FE pushed a new buffer
/// via `set_text` (CodeMirror typing).
#[derive(Debug)]
pub struct UpdateResult {
    pub parse_error:        Option<String>,
    pub root_kind:          Option<NodeKind>,
    pub child_count:        usize,
    pub can_undo:           bool,
    pub can_redo:           bool,
    /// Phase 3.d — recomputed on every `set_text` so the FE banner
    /// stays in sync with the live buffer.
    pub has_jsonc_features: bool,
}

/// Outcome of a structured mutation (`apply_mutation` / `undo` / `redo`).
/// Carries the regenerated text so the FE can refresh in one round-trip.
#[derive(Debug)]
pub struct MutateResult {
    pub text:               String,
    pub parse_error:        Option<String>,
    pub root_kind:          Option<NodeKind>,
    pub child_count:        usize,
    pub can_undo:           bool,
    pub can_redo:           bool,
    /// Phase 3.d — feature flag tracks the buffer the mutation produced.
    pub has_jsonc_features: bool,
}

/// One row in the lazy tree.
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

const PREVIEW_MAX_CHARS:    usize = 64;
const QUERY_MAX_HITS:       usize = 500;
const HISTORY_CAP:          usize = 200;

impl JsonStudioRegistry {
    /// Parse `text` and register a new doc. On parse error returns a
    /// `ParseResult` whose `parse_error` field carries the message;
    /// the doc is still registered (with a synthetic empty-object AST /
    /// no `value`) so the modal renders the raw text view + lets the
    /// user fix the input.
    ///
    /// Phase 3.d: `stream_threshold_bytes` decides between tree mode
    /// (`jsonc-parser` AST + full editing) and stream mode (`simd_json`
    /// strict, navigation-only). The extension of `source_path` (`.jsonc`
    /// vs everything else) drives the lenient/strict choice in tree
    /// mode — see `try_parse_tree`.
    pub fn parse(
        &mut self,
        text:                   String,
        source_path:            Option<String>,
        encoding_label:         String,
        had_bom:                bool,
        stream_threshold_bytes: usize,
    ) -> ParseResult {
        let size       = text.len();
        let is_jsonc   = source_path
            .as_deref()
            .map(is_jsonc_path)
            .unwrap_or(false);
        let parse_mode = if size >= stream_threshold_bytes {
            DocParseMode::Stream
        } else {
            DocParseMode::Tree
        };
        let (ast, value, parse_error, has_jsonc_features) =
            parse_pair(&text, parse_mode, is_jsonc);
        let kind        = ast.as_ref().map(kind_for_ast)
            .or_else(|| value.as_ref().map(kind_for_value));
        let child_count = ast.as_ref().map(|a| a.child_count())
            .or_else(|| value.as_ref().map(child_count_for_value))
            .unwrap_or(0);
        let indent      = detect_indent(&text);
        let id          = Uuid::new_v4().to_string();
        self.docs.insert(id.clone(), Doc {
            original:       text.clone(),
            current:        text.clone(),
            pretty:         None,
            ast,
            value,
            parse_error:    parse_error.clone(),
            indent,
            parse_mode,
            is_jsonc,
            has_jsonc_features,
            source_path:    source_path.clone(),
            encoding_label,
            had_bom,
            history:        History::new(text, HISTORY_CAP),
        });
        ParseResult {
            doc_id:      id,
            size_bytes:  size,
            root_kind:   kind,
            child_count,
            source_path,
            parse_error,
            parse_mode,
            is_jsonc,
            has_jsonc_features,
        }
    }

    pub fn close(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    fn doc(&self, doc_id: &str) -> Result<&Doc> {
        self.docs.get(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown JSON Studio doc: {doc_id}")))
    }

    fn doc_mut(&mut self, doc_id: &str) -> Result<&mut Doc> {
        self.docs.get_mut(doc_id)
            .ok_or_else(|| AppError::Other(format!("Unknown JSON Studio doc: {doc_id}")))
    }

    // ── Resolution ─────────────────────────────────────────────────

    fn resolve<'a>(&'a self, doc_id: &str, path: &[String]) -> Result<&'a Value> {
        let doc = self.doc(doc_id)?;
        let root = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot navigate".into()))?;
        let mut cur = root;
        for seg in path {
            cur = match cur {
                Value::Object(map) => map.get(seg)
                    .ok_or_else(|| AppError::Other(format!("Missing key: {seg}")))?,
                Value::Array(arr) => {
                    let idx: usize = seg.parse()
                        .map_err(|_| AppError::Other(format!("Invalid array index: {seg}")))?;
                    arr.get(idx)
                        .ok_or_else(|| AppError::Other(format!("Array index out of bounds: {idx}")))?
                }
                _ => return Err(AppError::Other(format!("Cannot descend into leaf at: {seg}"))),
            };
        }
        Ok(cur)
    }

    pub fn get_root(&self, doc_id: &str) -> Result<NodeView> {
        let v = self.resolve(doc_id, &[])?;
        Ok(view_for("$".to_string(), Vec::new(), v))
    }

    pub fn get_children(&self, doc_id: &str, path: &[String]) -> Result<Vec<NodeView>> {
        let v = self.resolve(doc_id, path)?;
        Ok(children_of(path, v))
    }

    pub fn get_value_pretty(&self, doc_id: &str, path: &[String]) -> Result<String> {
        let v = self.resolve(doc_id, path)?;
        serde_json::to_string_pretty(v).map_err(|e| AppError::Other(e.to_string()))
    }

    pub fn pretty(&mut self, doc_id: &str) -> Result<String> {
        let doc = self.doc_mut(doc_id)?;
        if let Some(t) = &doc.pretty { return Ok(t.clone()); }
        let v = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot pretty-print".into()))?;
        // Use the doc's configured indent (default 2 spaces). `serde_json`
        // hard-codes 2-space pretty output via `to_string_pretty`; for
        // other widths we serialise through a configured Serializer.
        let s = if doc.indent == "  " {
            serde_json::to_string_pretty(v).map_err(|e| AppError::Other(e.to_string()))?
        } else {
            let mut buf  = Vec::new();
            let fmt      = serde_json::ser::PrettyFormatter::with_indent(doc.indent.as_bytes());
            let mut ser  = serde_json::Serializer::with_formatter(&mut buf, fmt);
            v.serialize(&mut ser).map_err(|e| AppError::Other(e.to_string()))?;
            String::from_utf8(buf).map_err(|e| AppError::Other(e.to_string()))?
        };
        doc.pretty = Some(s.clone());
        Ok(s)
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
        Ok(self.doc(doc_id)?.ast.as_ref().map(kind_for_ast))
    }

    pub fn root_child_count(&self, doc_id: &str) -> Result<usize> {
        Ok(self.doc(doc_id)?.ast.as_ref().map(|a| a.child_count()).unwrap_or(0))
    }

    pub fn get_indent(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.indent.clone())
    }

    pub fn set_indent(&mut self, doc_id: &str, indent: String) -> Result<()> {
        let d = self.doc_mut(doc_id)?;
        d.indent = indent;
        d.pretty = None;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn current_text(&self, doc_id: &str) -> Result<String> {
        Ok(self.doc(doc_id)?.current.clone())
    }

    // ── Phase 3.d accessors ────────────────────────────────────────

    #[allow(dead_code)]
    pub fn parse_mode(&self, doc_id: &str) -> Result<DocParseMode> {
        Ok(self.doc(doc_id)?.parse_mode)
    }

    #[allow(dead_code)]
    pub fn is_jsonc(&self, doc_id: &str) -> Result<bool> {
        Ok(self.doc(doc_id)?.is_jsonc)
    }

    #[allow(dead_code)]
    pub fn has_jsonc_features(&self, doc_id: &str) -> Result<bool> {
        Ok(self.doc(doc_id)?.has_jsonc_features)
    }

    /// Strip JSONC-only constructs (comments + trailing commas) from the
    /// current buffer by reparsing lenient and re-emitting through
    /// `serde_json::to_string_pretty`. The result is wired through the
    /// normal `set_text` history path so undo/redo restore the original
    /// JSONC formatting. Returns the new buffer text. No-op (returns
    /// current text unchanged) when there's nothing to strip.
    pub fn strip_jsonc_features(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        if doc.parse_mode == DocParseMode::Stream {
            return Err(AppError::Other(
                "Large file (stream mode) — JSONC features not supported here.".into()
            ));
        }
        // Re-parse the current buffer lenient so we capture trailing
        // commas / comments even if `is_jsonc=false` (the buffer might
        // have JSONC features the user explicitly added).
        let ast = ast::parse_with(&doc.current, /* strict */ false)
            .map_err(|e| AppError::Other(format!("JSONC parse: {e}")))?;
        let value = ast::ast_to_value(&ast);
        // Pretty-print with the doc's configured indent.
        let pretty = if doc.indent == "  " {
            serde_json::to_string_pretty(&value)
                .map_err(|e| AppError::Other(e.to_string()))?
        } else {
            let mut buf = Vec::new();
            let fmt     = serde_json::ser::PrettyFormatter::with_indent(doc.indent.as_bytes());
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
            value.serialize(&mut ser).map_err(|e| AppError::Other(e.to_string()))?;
            String::from_utf8(buf).map_err(|e| AppError::Other(e.to_string()))?
        };
        // Discrete entry (no coalescing), one undo step for the reformat.
        doc.history.record_struct(pretty.clone());
        let mode      = doc.parse_mode;
        let is_jsonc  = doc.is_jsonc;
        let (new_ast, new_value, parse_error, has_jsonc_features) =
            parse_pair(&pretty, mode, is_jsonc);
        let kind        = new_ast.as_ref().map(kind_for_ast);
        let child_count = new_ast.as_ref().map(|a| a.child_count()).unwrap_or(0);
        doc.current            = pretty.clone();
        doc.ast                = new_ast;
        doc.value              = new_value;
        doc.parse_error        = parse_error.clone();
        doc.pretty             = None;
        doc.has_jsonc_features = has_jsonc_features;
        let can_undo = doc.history.can_undo();
        let can_redo = doc.history.can_redo();
        Ok(MutateResult {
            text:               pretty,
            parse_error,
            root_kind:          kind,
            child_count,
            can_undo,
            can_redo,
            has_jsonc_features,
        })
    }

    pub fn history_state(&self, doc_id: &str) -> Result<(bool, bool)> {
        let d = self.doc(doc_id)?;
        Ok((d.history.can_undo(), d.history.can_redo()))
    }

    // ── Editing — text level ───────────────────────────────────────

    /// Apply a raw-text edit (typing in the CodeMirror text pane).
    /// Coalesces consecutive edits within the history coalesce window so
    /// a burst of keystrokes produces one undoable entry instead of one
    /// per character.
    pub fn set_text(&mut self, doc_id: &str, text: String) -> Result<UpdateResult> {
        let doc = self.doc_mut(doc_id)?;
        let mode      = doc.parse_mode;
        let is_jsonc  = doc.is_jsonc;
        let (ast, value, parse_error, has_jsonc_features) =
            parse_pair(&text, mode, is_jsonc);
        let root_kind   = ast.as_ref().map(kind_for_ast)
            .or_else(|| value.as_ref().map(kind_for_value));
        let child_count = ast.as_ref().map(|a| a.child_count())
            .or_else(|| value.as_ref().map(child_count_for_value))
            .unwrap_or(0);
        doc.history.record_text(text.clone());
        doc.current            = text;
        doc.ast                = ast;
        doc.value              = value;
        doc.parse_error        = parse_error.clone();
        doc.pretty             = None;
        doc.has_jsonc_features = has_jsonc_features;
        let can_undo = doc.history.can_undo();
        let can_redo = doc.history.can_redo();
        Ok(UpdateResult {
            parse_error,
            root_kind,
            child_count,
            can_undo,
            can_redo,
            has_jsonc_features,
        })
    }

    // ── Editing — structural mutations ─────────────────────────────

    fn mutate_with<F>(&mut self, doc_id: &str, op: F) -> Result<MutateResult>
    where
        F: FnOnce(&str, &ast::JsonAst) -> Result<String>,
    {
        let doc = self.doc_mut(doc_id)?;
        if doc.parse_mode == DocParseMode::Stream {
            return Err(AppError::Other(
                "Large file (stream mode) — structural mutations are disabled. \
                 Switch to the text pane to edit raw bytes.".into()
            ));
        }
        let ast = doc.ast.as_ref().ok_or_else(|| AppError::Other(
            "Document has parse errors — cannot edit tree".into()
        ))?;
        let new_text = op(&doc.current, ast)?;
        // Validate before committing — if the splice produced invalid
        // JSON the mutation aborts and the caller never sees a corrupt
        // buffer. This is rare in practice (every helper splices syntax-
        // valid fragments) but cheap defense.
        let mode     = doc.parse_mode;
        let is_jsonc = doc.is_jsonc;
        let (new_ast, new_value, parse_error, has_jsonc_features) =
            parse_pair(&new_text, mode, is_jsonc);
        if parse_error.is_some() {
            return Err(AppError::Other(format!(
                "Mutation produced invalid JSON: {}",
                parse_error.unwrap()
            )));
        }
        // Structural mutations are discrete actions — never coalesce.
        doc.history.record_struct(new_text.clone());
        let kind        = new_ast.as_ref().map(kind_for_ast);
        let child_count = new_ast.as_ref().map(|a| a.child_count()).unwrap_or(0);
        doc.current            = new_text.clone();
        doc.ast                = new_ast;
        doc.value              = new_value;
        doc.parse_error        = None;
        doc.pretty             = None;
        doc.has_jsonc_features = has_jsonc_features;
        let can_undo = doc.history.can_undo();
        let can_redo = doc.history.can_redo();
        Ok(MutateResult {
            text:               new_text,
            parse_error:        None,
            root_kind:          kind,
            child_count,
            can_undo,
            can_redo,
            has_jsonc_features,
        })
    }

    pub fn mutate_primitive(
        &mut self,
        doc_id: &str,
        path:   &[String],
        value:  Value,
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::set_primitive(text, ast, path, &value))
    }

    pub fn replace_at(
        &mut self,
        doc_id: &str,
        path:   &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::replace_at(text, ast, path, &snippet))
    }

    pub fn remove_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::remove_at(text, ast, path))
    }

    pub fn insert_field(
        &mut self,
        doc_id: &str,
        path:   &[String],
        name:   String,
        snippet: String,
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::insert_field(text, ast, path, &name, &snippet))
    }

    pub fn insert_item(
        &mut self,
        doc_id: &str,
        path:   &[String],
        snippet: String,
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::insert_item(text, ast, path, &snippet))
    }

    pub fn insert_map_entry(
        &mut self,
        doc_id:   &str,
        path:     &[String],
        key_text: String,
        val_text: String,
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| {
            edits::insert_map_entry(text, ast, path, &key_text, &val_text)
        })
    }

    pub fn duplicate_at(&mut self, doc_id: &str, path: &[String]) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::duplicate_at(text, ast, path))
    }

    pub fn move_item(
        &mut self,
        doc_id: &str,
        path:   &[String],
        delta:  i32,
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, ast| edits::move_item(text, ast, path, delta))
    }

    /// F13 — apply a batch of `JsonBulkOp`s to an open doc. Recorded as
    /// a single discrete history entry (one bulk edit = one undo).
    /// `mutate_with`'s validate-then-commit pipeline rolls back the
    /// buffer if any op produces invalid JSON.
    pub fn apply_bulk_edits_doc(
        &mut self,
        doc_id: &str,
        ops:    &[(Vec<String>, bulk_edits::JsonBulkOp)],
    ) -> Result<MutateResult> {
        self.mutate_with(doc_id, |text, _ast| {
            bulk_edits::apply_bulk_edits_text(text, ops)
        })
    }

    // ── Undo / redo ────────────────────────────────────────────────

    pub fn undo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        let text = doc.history.undo()
            .ok_or_else(|| AppError::Other("Nothing to undo".into()))?
            .clone();
        Self::apply_history_cursor(doc, text)
    }

    pub fn redo(&mut self, doc_id: &str) -> Result<MutateResult> {
        let doc = self.doc_mut(doc_id)?;
        let text = doc.history.redo()
            .ok_or_else(|| AppError::Other("Nothing to redo".into()))?
            .clone();
        Self::apply_history_cursor(doc, text)
    }

    fn apply_history_cursor(doc: &mut Doc, text: String) -> Result<MutateResult> {
        let mode     = doc.parse_mode;
        let is_jsonc = doc.is_jsonc;
        let (ast, value, parse_error, has_jsonc_features) =
            parse_pair(&text, mode, is_jsonc);
        let kind        = ast.as_ref().map(kind_for_ast)
            .or_else(|| value.as_ref().map(kind_for_value));
        let child_count = ast.as_ref().map(|a| a.child_count())
            .or_else(|| value.as_ref().map(child_count_for_value))
            .unwrap_or(0);
        doc.current            = text.clone();
        doc.ast                = ast;
        doc.value              = value;
        doc.parse_error        = parse_error.clone();
        doc.pretty             = None;
        doc.has_jsonc_features = has_jsonc_features;
        let can_undo = doc.history.can_undo();
        let can_redo = doc.history.can_redo();
        Ok(MutateResult {
            text,
            parse_error,
            root_kind: kind,
            child_count,
            can_undo,
            can_redo,
            has_jsonc_features,
        })
    }

    // ── Diff ───────────────────────────────────────────────────────

    /// Unified text-diff between `original` and `current`. Empty `Vec`
    /// when the buffer is pristine. Same shape RON / the other formats
    /// emit so the shared `StudioDiffPane` renders without per-format
    /// branching.
    pub fn diff(&self, doc_id: &str) -> Result<Vec<DiffHunk>> {
        let doc = self.doc(doc_id)?;
        Ok(unified_diff(&doc.original, &doc.current))
    }

    /// Tree-shaped diff. Compares the original AST (parsed lazily here)
    /// against the current AST recursively, pruning unchanged branches.
    /// Stream-mode docs return an empty (Unchanged-root) diff since
    /// they don't carry an AST — the text diff still works via `diff()`.
    pub fn tree_diff(&self, doc_id: &str) -> Result<DiffTreeNode> {
        let doc = self.doc(doc_id)?;
        if doc.parse_mode == DocParseMode::Stream {
            return Ok(build_tree_diff(None, None));
        }
        // Parse the original lenient too — `.jsonc` originals must
        // round-trip even though the tree-diff doesn't show comments.
        let orig_ast = ast::parse_with(&doc.original, /* strict */ false).ok();
        let curr_ast = doc.ast.clone();
        Ok(build_tree_diff(orig_ast.as_ref(), curr_ast.as_ref()))
    }

    // ── Save ───────────────────────────────────────────────────────

    /// Mark the doc clean: `current` becomes the new `original`. Used
    /// after a successful disk write so the dirty flag drops.
    pub fn mark_saved(&mut self, doc_id: &str) -> Result<()> {
        let doc = self.doc_mut(doc_id)?;
        doc.original = doc.current.clone();
        Ok(())
    }

    /// Rebind the doc to a new on-disk source (Save As flow).
    pub fn rebind_source(&mut self, doc_id: &str, path: String) -> Result<()> {
        let doc = self.doc_mut(doc_id)?;
        doc.source_path = Some(path);
        Ok(())
    }

    // ── Query (JSON-Path) ──────────────────────────────────────────

    /// F13 helper — run a JSON-Path query against the doc's parsed
    /// `Value` and return `(path, value)` pairs. The owned `Value`
    /// clones avoid lifetime ties to the registry so callers can hold
    /// them across other registry calls.
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

    pub fn query(&self, doc_id: &str, expr: &str) -> Result<Vec<QueryHit>> {
        let doc = self.doc(doc_id)?;
        let root = doc.value.as_ref()
            .ok_or_else(|| AppError::Other("Document has parse errors — cannot query".into()))?;
        let locs = arbor_studio_core::prelude::query::run(root, expr, QUERY_MAX_HITS)
            .map_err(|e| AppError::Other(e.0))?;
        Ok(locs.into_iter().map(|loc| QueryHit {
            kind:    node_kind(&loc.value),
            preview: preview_for(&loc.value),
            path:    loc.path,
        }).collect())
    }
}

// ── F12 — Cross-reference rename (lossless) ──────────────────────────────────

/// Splice `new_value` over every JSON string node at the given paths,
/// preserving every byte outside the targeted spans (FROZEN F11/F17:
/// JSON Studio guarantees lossless edits). The new value is JSON-
/// encoded (surrounding quotes + escape sequences) before splicing —
/// the AST spans cover the quoted literal, so the replacement must
/// be a complete JSON string literal.
///
/// Paths that don't resolve to a string node are reported as an error
/// before any splice happens (pre-flush validation — matches F12's
/// atomic-by-file contract).
///
/// Applies splices in reverse document order so earlier splices never
/// invalidate the byte spans of later ones.
pub fn apply_string_rename(
    text:      &str,
    paths:     &[Vec<String>],
    new_value: &str,
) -> Result<String> {
    // Phase 3.d: parse lenient so `.jsonc` source files (with comments
    // / trailing commas) round-trip through F12 cross-file rename. The
    // splice itself is byte-level so any preserved JSONC features
    // outside the touched spans survive intact.
    let root = ast::parse_with(text, /* strict */ false)
        .map_err(|e| AppError::Other(format!("parse: {e}")))?;

    // Resolve every site up-front so a failure on any path aborts
    // before we mutate anything (mirrors RON's pre-flush validation).
    let mut spans: Vec<ast::Span> = Vec::with_capacity(paths.len());
    for path in paths {
        let target = ast::resolve(&root, path)
            .ok_or_else(|| AppError::Other(format!(
                "Rename site path not found: {}", path.join("/"),
            )))?;
        match target {
            ast::JsonAst::String(s) => spans.push(s.span),
            other => return Err(AppError::Other(format!(
                "Rename site at {path:?} is not a string leaf (kind = {})",
                other.kind_str(),
            ))),
        }
    }
    let lit = serde_json::to_string(new_value)
        .map_err(|e| AppError::Other(format!("encode string literal: {e}")))?;

    // Reverse document order — splicing from the end keeps earlier
    // spans valid.
    spans.sort_by(|a, b| b.start.cmp(&a.start));
    let mut out = text.to_string();
    for span in spans {
        let mut next = String::with_capacity(out.len() + lit.len());
        next.push_str(&out[..span.start]);
        next.push_str(&lit);
        next.push_str(&out[span.end..]);
        out = next;
    }
    Ok(out)
}

// ── F13 query against arbitrary Value (used by project-wide flow) ───────────

/// Run a JSON-Path expression against `root` and return owned
/// `(path, value)` pairs. `path` segments mirror the wire format used
/// elsewhere (object keys as-is, array indices as decimal strings).
pub fn query_value_pairs_against(
    root: &Value,
    expr: &str,
) -> Result<Vec<(Vec<String>, Value)>> {
    let locs = arbor_studio_core::prelude::query::run(root, expr, QUERY_MAX_HITS)
        .map_err(|e| AppError::Other(e.0))?;
    Ok(locs.into_iter().map(|loc| (loc.path, loc.value)).collect())
}

/// `kind_str` helper exposed for the bulk-edit preview path which
/// builds `BulkEditSite { kind: String }` without going through the
/// AST → NodeKind → string chain.
pub fn json_kind_str(v: &Value) -> &'static str {
    match v {
        Value::Object(_) => "object",
        Value::Array(_)  => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_)   => "bool",
        Value::Null      => "null",
    }
}

/// Pretty-preview shared with the bulk-edit site builder so the modal's
/// old-line/new-line look consistent with the tree pane.
pub fn json_preview_for(v: &Value) -> String { preview_for(v) }

// ── On-disk write (mirrors RON's `write_to_disk`) ───────────────────────────

/// Write `contents` to `path` using `encoding_label` + BOM hint
/// (FROZEN F16). Creates parent dirs if missing. Same shape RON's
/// `write_to_disk` exposes — the trait's `save` method funnels through
/// here.
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
    let bytes = arbor_fs::prelude::encoding::encode_for_disk_with_bom(
        contents,
        Some(encoding_label),
        had_bom,
    );
    std::fs::write(path, &bytes).map_err(|e| AppError::Other(format!("write {path}: {e}")))
}

// ── Parse helpers ───────────────────────────────────────────────────────────

/// Returns `true` when `path` looks like a JSONC file by extension.
/// Case-insensitive, tolerates trailing path separators / spaces.
pub fn is_jsonc_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.trim_end_matches(['/', '\\', ' '])
        .ends_with(".jsonc")
}

/// Parse `text` according to the doc's mode + extension.
///
/// - **Tree** mode: try the strict `jsonc-parser` first when the doc is
///   `.json`; fall back to lenient on syntax errors and flag jsonc
///   features so the FE shows the "rename to .jsonc / strip" banner.
///   `.jsonc` always parses lenient.
///   The navigation `Value` is built from the AST (lossless mapping)
///   instead of going through `simd_json` strict — comments / trailing
///   commas would otherwise stop it.
/// - **Stream** mode: skip AST construction entirely, parse via
///   `simd_json` strict only. `has_jsonc_features` is always `false`
///   in this mode (the strict parser would have errored on them).
///
/// Returns `(ast, value, parse_error, has_jsonc_features)`. The error
/// message wins from the parser that actually ran (strict-first when
/// `.json` tree-mode).
fn parse_pair(
    text:     &str,
    mode:     DocParseMode,
    is_jsonc: bool,
) -> (Option<ast::JsonAst>, Option<Value>, Option<String>, bool) {
    match mode {
        DocParseMode::Stream => {
            let mut bytes = text.as_bytes().to_vec();
            match simd_json::serde::from_slice::<Value>(&mut bytes) {
                Ok(v)  => (None, Some(v), None, false),
                Err(e) => (None, None, Some(format!("JSON parse error: {e}")), false),
            }
        }
        DocParseMode::Tree => {
            let (ast_opt, has_features, err_msg) = if is_jsonc {
                match ast::parse_with(text, /* strict */ false) {
                    Ok(a)  => (Some(a), ast::detect_jsonc_features(text), None),
                    Err(e) => (None, false, Some(e)),
                }
            } else {
                // `.json` — try strict first; if it fails AND lenient
                // would have succeeded, we treat the buffer as "JSONC
                // accidentally" and flag features so the banner fires.
                match ast::parse_with(text, /* strict */ true) {
                    Ok(a)  => (Some(a), ast::detect_jsonc_features(text), None),
                    Err(strict_err) => match ast::parse_with(text, false) {
                        Ok(a)  => (Some(a), true, None),
                        Err(_) => (None, false, Some(strict_err)),
                    },
                }
            };
            let value_opt = ast_opt.as_ref().map(ast::ast_to_value);
            (ast_opt, value_opt, err_msg, has_features)
        }
    }
}

/// Map `serde_json::Value` to a `NodeKind` for the stream-mode root
/// readout (no AST is built so `kind_for_ast` can't be used).
fn kind_for_value(v: &Value) -> NodeKind {
    match v {
        Value::Object(_) => NodeKind::Object,
        Value::Array(_)  => NodeKind::Array,
        Value::String(_) => NodeKind::String,
        Value::Number(_) => NodeKind::Number,
        Value::Bool(_)   => NodeKind::Bool,
        Value::Null      => NodeKind::Null,
    }
}

fn child_count_for_value(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.len(),
        Value::Array(a)  => a.len(),
        _ => 0,
    }
}

/// Detect the indent the doc was opened with. Looks at the leading
/// whitespace of the first prop/item under the root. Defaults to two
/// spaces.
fn detect_indent(text: &str) -> String {
    // Find the first `\n` followed by `[ \t]+` followed by `"` or `{`.
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '\n' {
            let mut ws = String::new();
            let mut saw_nonws = false;
            while let Some(&(_, n)) = chars.peek() {
                if n == ' ' || n == '\t' {
                    ws.push(n);
                    chars.next();
                } else {
                    saw_nonws = true;
                    break;
                }
            }
            if saw_nonws && !ws.is_empty() { return ws; }
        }
    }
    "  ".into()
}

// ── Diff helpers ────────────────────────────────────────────────────────────

fn unified_diff(original: &str, current: &str) -> Vec<DiffHunk> {
    arbor_studio_core::prelude::diff::unified(original, current)
}

fn build_tree_diff(
    orig: Option<&ast::JsonAst>,
    curr: Option<&ast::JsonAst>,
) -> DiffTreeNode {
    match (orig, curr) {
        (Some(a), Some(b)) => walk_tree_diff("$".into(), Vec::new(), Some(a), Some(b)),
        (Some(a), None)    => leaf_diff("$".into(), Vec::new(), Some(a), None,    DiffStatus::Removed),
        (None,    Some(b)) => leaf_diff("$".into(), Vec::new(), None,    Some(b), DiffStatus::Added),
        (None,    None)    => DiffTreeNode {
            key:             "$".into(),
            path:            Vec::new(),
            status:          DiffStatus::Unchanged,
            kind_before:     None,
            kind_after:      None,
            preview_before:  None,
            preview_after:   None,
            tag_before:      None,
            tag_after:       None,
            children:        Vec::new(),
            change_count:    0,
        },
    }
}

fn walk_tree_diff(
    key:  String,
    path: Vec<String>,
    a:    Option<&ast::JsonAst>,
    b:    Option<&ast::JsonAst>,
) -> DiffTreeNode {
    match (a, b) {
        (Some(a), Some(b)) if same_shape(a, b) => match (a, b) {
            (ast::JsonAst::Object(ao), ast::JsonAst::Object(bo)) => {
                tree_diff_object(key, path, ao, bo)
            }
            (ast::JsonAst::Array(aa), ast::JsonAst::Array(ba)) => {
                tree_diff_array(key, path, aa, ba)
            }
            _ => {
                let equal = leaf_equal(a, b);
                let status = if equal { DiffStatus::Unchanged } else { DiffStatus::Modified };
                DiffTreeNode {
                    key,
                    path,
                    status,
                    kind_before:    Some(a.kind_str().to_string()),
                    kind_after:     Some(b.kind_str().to_string()),
                    preview_before: Some(ast::preview(a)),
                    preview_after:  Some(ast::preview(b)),
                    tag_before:     None,
                    tag_after:      None,
                    children:       Vec::new(),
                    change_count:   if equal { 0 } else { 1 },
                }
            }
        },
        (Some(a), Some(b)) => DiffTreeNode {
            key,
            path,
            status:         DiffStatus::Modified,
            kind_before:    Some(a.kind_str().to_string()),
            kind_after:     Some(b.kind_str().to_string()),
            preview_before: Some(ast::preview(a)),
            preview_after:  Some(ast::preview(b)),
            tag_before:     None,
            tag_after:      None,
            children:       Vec::new(),
            change_count:   1,
        },
        (Some(a), None) => leaf_diff(key, path, Some(a), None, DiffStatus::Removed),
        (None, Some(b)) => leaf_diff(key, path, None, Some(b), DiffStatus::Added),
        (None, None) => DiffTreeNode {
            key,
            path,
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

fn tree_diff_object(
    key:  String,
    path: Vec<String>,
    a:    &ast::JsonObject,
    b:    &ast::JsonObject,
) -> DiffTreeNode {
    let mut children = Vec::new();
    // Union the key sets, preserving the order of b (current) — that's
    // what the user sees, so the diff tree shows children in the same
    // order.
    let mut seen = std::collections::HashSet::<String>::new();
    for bp in &b.props {
        let ap = a.props.iter().find(|p| p.name == bp.name);
        let child_path = {
            let mut p = path.clone();
            p.push(bp.name.clone());
            p
        };
        let node = walk_tree_diff(
            bp.name.clone(),
            child_path,
            ap.map(|p| &p.value),
            Some(&bp.value),
        );
        if node.status != DiffStatus::Unchanged {
            children.push(node);
        }
        seen.insert(bp.name.clone());
    }
    // Removed props (present in a, missing in b).
    for ap in &a.props {
        if seen.contains(&ap.name) { continue; }
        let child_path = {
            let mut p = path.clone();
            p.push(ap.name.clone());
            p
        };
        children.push(walk_tree_diff(
            ap.name.clone(),
            child_path,
            Some(&ap.value),
            None,
        ));
    }
    let change_count: u32 = children.iter().map(|c| c.change_count).sum();
    let status = if change_count == 0 { DiffStatus::Unchanged } else { DiffStatus::Partial };
    DiffTreeNode {
        key,
        path,
        status,
        kind_before:    Some("object".into()),
        kind_after:     Some("object".into()),
        preview_before: Some(format!("{{{} keys}}", a.props.len())),
        preview_after:  Some(format!("{{{} keys}}", b.props.len())),
        tag_before:     None,
        tag_after:      None,
        children,
        change_count,
    }
}

fn tree_diff_array(
    key:  String,
    path: Vec<String>,
    a:    &ast::JsonArray,
    b:    &ast::JsonArray,
) -> DiffTreeNode {
    let mut children = Vec::new();
    let max = a.items.len().max(b.items.len());
    for i in 0..max {
        let ai = a.items.get(i);
        let bi = b.items.get(i);
        let child_path = {
            let mut p = path.clone();
            p.push(i.to_string());
            p
        };
        let node = walk_tree_diff(i.to_string(), child_path, ai, bi);
        if node.status != DiffStatus::Unchanged {
            children.push(node);
        }
    }
    let change_count: u32 = children.iter().map(|c| c.change_count).sum();
    let status = if change_count == 0 { DiffStatus::Unchanged } else { DiffStatus::Partial };
    DiffTreeNode {
        key,
        path,
        status,
        kind_before:    Some("array".into()),
        kind_after:     Some("array".into()),
        preview_before: Some(format!("[{} items]", a.items.len())),
        preview_after:  Some(format!("[{} items]", b.items.len())),
        tag_before:     None,
        tag_after:      None,
        children,
        change_count,
    }
}

fn leaf_diff(
    key:    String,
    path:   Vec<String>,
    a:      Option<&ast::JsonAst>,
    b:      Option<&ast::JsonAst>,
    status: DiffStatus,
) -> DiffTreeNode {
    DiffTreeNode {
        key,
        path,
        status,
        kind_before:    a.map(|x| x.kind_str().to_string()),
        kind_after:     b.map(|x| x.kind_str().to_string()),
        preview_before: a.map(ast::preview),
        preview_after:  b.map(ast::preview),
        tag_before:     None,
        tag_after:      None,
        children:       Vec::new(),
        change_count:   1,
    }
}

fn same_shape(a: &ast::JsonAst, b: &ast::JsonAst) -> bool {
    matches!(
        (a, b),
        (ast::JsonAst::Object(_), ast::JsonAst::Object(_))
            | (ast::JsonAst::Array(_),  ast::JsonAst::Array(_))
            | (ast::JsonAst::String(_), ast::JsonAst::String(_))
            | (ast::JsonAst::Number(_), ast::JsonAst::Number(_))
            | (ast::JsonAst::Bool(_),   ast::JsonAst::Bool(_))
            | (ast::JsonAst::Null(_),   ast::JsonAst::Null(_))
    )
}

fn leaf_equal(a: &ast::JsonAst, b: &ast::JsonAst) -> bool {
    match (a, b) {
        (ast::JsonAst::String(x), ast::JsonAst::String(y)) => x.value == y.value,
        (ast::JsonAst::Number(x), ast::JsonAst::Number(y)) => x.raw   == y.raw,
        (ast::JsonAst::Bool(x),   ast::JsonAst::Bool(y))   => x.value == y.value,
        (ast::JsonAst::Null(_),   ast::JsonAst::Null(_))   => true,
        _ => false,
    }
}

// ── Misc helpers ────────────────────────────────────────────────────────────

fn kind_for_ast(a: &ast::JsonAst) -> NodeKind {
    match a {
        ast::JsonAst::Object(_) => NodeKind::Object,
        ast::JsonAst::Array(_)  => NodeKind::Array,
        ast::JsonAst::String(_) => NodeKind::String,
        ast::JsonAst::Number(_) => NodeKind::Number,
        ast::JsonAst::Bool(_)   => NodeKind::Bool,
        ast::JsonAst::Null(_)   => NodeKind::Null,
    }
}

fn node_kind(v: &Value) -> NodeKind {
    match v {
        Value::Object(_) => NodeKind::Object,
        Value::Array(_)  => NodeKind::Array,
        Value::String(_) => NodeKind::String,
        Value::Number(_) => NodeKind::Number,
        Value::Bool(_)   => NodeKind::Bool,
        Value::Null      => NodeKind::Null,
    }
}

fn container_len(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.len(),
        Value::Array(a)  => a.len(),
        _ => 0,
    }
}

fn preview_for(v: &Value) -> String {
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

fn view_for(key: String, path: Vec<String>, v: &Value) -> NodeView {
    NodeView {
        key,
        path,
        kind:        node_kind(v),
        preview:     preview_for(v),
        child_count: container_len(v),
    }
}

fn children_of(parent_path: &[String], v: &Value) -> Vec<NodeView> {
    match v {
        Value::Object(map) => {
            let mut items: Vec<NodeView> = map.iter().map(|(k, child)| {
                let mut p = parent_path.to_vec();
                p.push(k.clone());
                view_for(k.clone(), p, child)
            }).collect();
            items.sort_by(|a, b| {
                let a_leaf = a.child_count == 0;
                let b_leaf = b.child_count == 0;
                a_leaf.cmp(&b_leaf)
            });
            items
        }
        Value::Array(arr) => arr.iter().enumerate().map(|(i, child)| {
            let key = i.to_string();
            let mut p = parent_path.to_vec();
            p.push(key.clone());
            view_for(key, p, child)
        }).collect(),
        _ => Vec::new(),
    }
}

