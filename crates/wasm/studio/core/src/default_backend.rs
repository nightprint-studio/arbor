//! `core::default_backend` — [`DefaultBackend<F>`], the generic
//! `StudioFormatBackend` wrapper for "simple" formats (blueprint §2.6).
//!
//! This is the lever that deletes the ~450 LOC of registry + type-mapping
//! boilerplate copy-pasted ×3 across TOML / YAML / .properties. It
//! implements the full [`StudioFormatBackend`] trait (+ [`RefactorOps`])
//! once against the small [`SimpleFormat`] seam, owning ALL the shared
//! machinery:
//!
//! * the doc registry (`Mutex<HashMap<doc_id, DocState>>`),
//! * `History<String>` (cap 200; dedup is a construction flag — `.properties`
//!   wants it ON, JSON/TOML/YAML OFF),
//! * encoding label + BOM (FROZEN F16 round-trip on save),
//! * original / current text snapshots, parse-error, indent,
//!
//! and delegates the format-specific bits to `F` (parse / project / emit /
//! mutate / kind / preview) and the engines to `core::{history, diff,
//! query, refactor, persist}`.
//!
//! The trait's schema methods route to injected
//! `Arc<dyn SchemaProvider>` providers (see [`SchemaRouting`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use arbor_studio_types::prelude::{
    BulkEditAction, BulkEditOpenDoc, BulkEditPreview, BulkEditResult, BulkEditScope, BulkEditSite,
    BulkEditValueSource, CrateProbe, DiffHunk, DiffTreeNode, DocSnapshot, EncodingInfo, FileEntry,
    MutateResult, NodeView, ParseResult, QueryHit, RenameOpenDoc, RenameResult,
    RenameSite, Schema, StudioError, StudioMutation, StudioResult, TypeSource, UpdateResult,
};

use crate::backend::StudioFormatBackend;
use crate::refactor::{
    self, BulkOp, CoerceOutcome, CoerceSkip, OpenDocState, RefactorOps, SetValue,
};
use crate::schema::SchemaProvider;
use crate::simple::{SimpleFormat, SimpleMutation};
use crate::{diff, history::History, query};

const QUERY_MAX_HITS: usize = 500;
const HISTORY_CAP: usize = 200;

/// How the backend resolves a schema `source` path to a provider.
///
/// TOML declares both Rust + JSON schema sources, so it routes `.rs`
/// files to the Rust provider and everything else to the JSON provider.
/// YAML / .properties declare only JSON Schema, so they route everything
/// to a single provider. A format with no schema support uses
/// [`SchemaRouting::None`] (the trait defaults then surface
/// `Unsupported`).
#[derive(Clone, Default)]
pub enum SchemaRouting {
    /// No schema support.
    #[default]
    None,
    /// One provider for every source (YAML / .properties → JSON only).
    Single(Arc<dyn SchemaProvider>),
    /// Route `.rs` → `rust`, everything else → `other` (TOML).
    RustOrOther {
        rust:  Arc<dyn SchemaProvider>,
        other: Arc<dyn SchemaProvider>,
    },
}

impl SchemaRouting {
    /// Pick the provider for `source` (filename-only sniff: `.rs` →
    /// Rust). `None` when this format has no schema support.
    fn provider_for(&self, source: &str) -> Option<&Arc<dyn SchemaProvider>> {
        match self {
            SchemaRouting::None => None,
            SchemaRouting::Single(p) => Some(p),
            SchemaRouting::RustOrOther { rust, other } => {
                if source.to_ascii_lowercase().ends_with(".rs") {
                    Some(rust)
                } else {
                    Some(other)
                }
            }
        }
    }
}

/// Per-document state owned by the registry. Mirrors the `Doc` struct
/// each simple backend hand-rolled today, minus the format-native AST
/// (the backend caches only the projected `Value`; `F` re-parses its own
/// AST on each mutation — see `core::simple` module docs).
struct DocState {
    original:       String,
    current:        String,
    /// JSON projection of `current`. `None` when the buffer is
    /// unparseable.
    value:          Option<Value>,
    parse_error:    Option<String>,
    indent:         String,
    source_path:    Option<String>,
    encoding_label: String,
    had_bom:        bool,
    history:        History<String>,
}

/// Generic `StudioFormatBackend` for a [`SimpleFormat`]. Owns the doc
/// registry + history + encoding boilerplate; delegates format-specific
/// work to `F` and the engines to `core`.
pub struct DefaultBackend<F: SimpleFormat> {
    fmt:     F,
    schema:  SchemaRouting,
    /// `true` for `.properties` (replayed no-op snapshots must not
    /// pollute history); `false` for JSON/TOML/YAML.
    dedup:   bool,
    docs:    Mutex<HashMap<String, DocState>>,
}

impl<F: SimpleFormat> DefaultBackend<F> {
    /// Construct a backend for `fmt`.
    ///
    /// * `schema` injects the schema provider(s) per §3.5 (the registry
    ///   passes [`SchemaRouting::Single`] for JSON-only formats,
    ///   [`SchemaRouting::RustOrOther`] for TOML, [`SchemaRouting::None`]
    ///   for none).
    /// * `dedup` is the one genuine history divergence: `true` for
    ///   `.properties`, `false` for JSON/TOML/YAML.
    pub fn new(fmt: F, schema: SchemaRouting, dedup: bool) -> Self {
        Self { fmt, schema, dedup, docs: Mutex::new(HashMap::new()) }
    }

    fn lock(&self) -> StudioResult<std::sync::MutexGuard<'_, HashMap<String, DocState>>> {
        self.docs
            .lock()
            .map_err(|_| StudioError::App("studio doc registry poisoned".into()))
    }

    fn fmt_id(&self) -> &'static str {
        // Match the known descriptor ids so the registry/leaf borrows a
        // 'static str for `Unsupported` and the descriptor-id helper.
        match self.fmt.descriptor().id.as_str() {
            "toml"       => "toml",
            "yaml"       => "yaml",
            "properties" => "properties",
            _            => "unknown",
        }
    }

    fn make_history(&self, initial: String) -> History<String> {
        if self.dedup {
            History::new_dedup(initial, HISTORY_CAP)
        } else {
            History::new(initial, HISTORY_CAP)
        }
    }

    fn root_kind_of(&self, value: Option<&Value>) -> Option<String> {
        value.map(|v| self.fmt.node_kind(v))
    }

    fn child_count_of(value: Option<&Value>) -> usize {
        match value {
            Some(Value::Object(m)) => m.len(),
            Some(Value::Array(a))  => a.len(),
            _                      => 0,
        }
    }

    fn with_doc<R>(
        &self,
        doc_id: &str,
        f: impl FnOnce(&DocState) -> StudioResult<R>,
    ) -> StudioResult<R> {
        let guard = self.lock()?;
        let doc = guard
            .get(doc_id)
            .ok_or_else(|| StudioError::App(format!("Unknown studio doc: {doc_id}")))?;
        f(doc)
    }

    fn with_doc_mut<R>(
        &self,
        doc_id: &str,
        f: impl FnOnce(&mut DocState) -> StudioResult<R>,
    ) -> StudioResult<R> {
        let mut guard = self.lock()?;
        let doc = guard
            .get_mut(doc_id)
            .ok_or_else(|| StudioError::App(format!("Unknown studio doc: {doc_id}")))?;
        f(doc)
    }

    /// Re-parse `text`, write the projection + parse-error onto `doc`, and
    /// build the `(root_kind, child_count)` pair for a result DTO. Shared
    /// by `set_text`, `mutate`, undo/redo cursor application.
    fn refresh_projection(&self, doc: &mut DocState, text: String) -> (Option<String>, usize) {
        let outcome = self.fmt.parse(&text, &EncodingInfo {
            label:   doc.encoding_label.clone(),
            had_bom: doc.had_bom,
        });
        let root_kind   = self.root_kind_of(outcome.value.as_ref());
        let child_count = Self::child_count_of(outcome.value.as_ref());
        doc.current     = text;
        doc.value       = outcome.value;
        doc.parse_error = outcome.error;
        (root_kind, child_count)
    }

    /// Build a `MutateResult` reading the current parse state off `doc`.
    fn mutate_result(doc: &DocState, root_kind: Option<String>, child_count: usize) -> MutateResult {
        MutateResult {
            text:               doc.current.clone(),
            parse_error:        doc.parse_error.clone(),
            root_kind,
            child_count,
            can_undo:           doc.history.can_undo(),
            can_redo:           doc.history.can_redo(),
            has_jsonc_features: false,
        }
    }

    /// Resolve a value node by `path` from the doc's projection.
    fn resolve<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
        refactor::resolve_value_path(root, path)
    }

    // ── F13 active-doc op lowering (the only registry-coupled bulk bit) ─

    /// Run the F13 active-doc apply against the open doc: re-resolve the
    /// root, build ops, apply through the format's mutate seam as ONE
    /// history entry. Returns the active-doc `MutateResult` (or `None`
    /// when no op applied), plus the applied/skipped counts.
    fn bulk_apply_active(
        &self,
        doc_id:       &str,
        sites:        &[BulkEditSite],
        action:       BulkEditAction,
        value_source: &Option<BulkEditValueSource>,
        compiled:     Option<&crate::edit_expr::CompiledExpr>,
    ) -> StudioResult<(Option<MutateResult>, usize, usize)> {
        // Snapshot the current text + root projection under the lock.
        let (text, root) = self.with_doc(doc_id, |doc| {
            Ok((
                doc.current.clone(),
                doc.value.clone().unwrap_or(Value::Null),
            ))
        })?;

        let (ops, applied, skipped) =
            refactor::build_bulk_ops(self, &root, sites, action, value_source, compiled);

        if ops.is_empty() {
            return Ok((None, applied, skipped));
        }

        let new_text = self.apply_bulk_ops(&text, &ops)?;
        // One bulk edit = one undo step (structural record).
        let result = self.with_doc_mut(doc_id, |doc| {
            doc.history.record_struct(new_text.clone());
            let (rk, cc) = self.refresh_projection(doc, new_text);
            Ok(Self::mutate_result(doc, rk, cc))
        })?;
        Ok((Some(result), applied, skipped))
    }
}

// ─── RefactorOps — DefaultBackend IS the leaf for F12/F13 ──────────────
//
// The simple formats route F12/F13 through `core::refactor` against this
// impl. `parse_to_value` / `node_kind` / `preview_for` come straight from
// `F`. `apply_string_rename` is expressed as a batch of SetPrimitive(String)
// mutations through `F::mutate` (lossless where the format allows). The
// `apply_bulk_ops` lowering applies sets then deletes (reverse-index) via
// `F::mutate`. Coercion is delegated to the descriptor's null policy.

impl<F: SimpleFormat> RefactorOps for DefaultBackend<F> {
    fn parse_to_value(&self, text: &str) -> Option<Value> {
        self.fmt
            .parse(text, &EncodingInfo::utf8())
            .value
    }

    fn apply_string_rename(
        &self,
        text:  &str,
        paths: &[Vec<String>],
        new:   &str,
    ) -> StudioResult<String> {
        // Validate every site resolves to a string leaf BEFORE mutating
        // (pre-flush atomicity, matching the per-format apply_string_rename).
        let root = self
            .fmt
            .parse(text, &EncodingInfo::utf8())
            .value
            .ok_or_else(|| StudioError::App("parse failed during rename".into()))?;
        for path in paths {
            match Self::resolve(&root, path) {
                Some(Value::String(_)) => {}
                Some(_) => {
                    return Err(StudioError::App(format!(
                        "Rename site at {path:?} is not a string leaf",
                    )))
                }
                None => {
                    return Err(StudioError::App(format!(
                        "Rename site path not found: {}",
                        path.join("/"),
                    )))
                }
            }
        }
        // Apply each rename as a SetPrimitive(String). `F::mutate`
        // re-parses + re-emits per call; the format preserves decor where
        // it can (toml_edit / yaml_edit / line-view all do).
        let mut current = text.to_string();
        for path in paths {
            current = self.fmt.mutate(
                &current,
                SimpleMutation::SetPrimitive {
                    path:  path.clone(),
                    value: Value::String(new.to_string()),
                },
            )?;
        }
        Ok(current)
    }

    fn apply_bulk_ops(&self, text: &str, ops: &[BulkOp]) -> StudioResult<String> {
        let mut current = text.to_string();
        // Phase A — sets (order irrelevant among sets).
        for op in ops {
            if let BulkOp::Set { path, value } = op {
                current = self.fmt.mutate(
                    &current,
                    SimpleMutation::SetPrimitive {
                        path:  path.clone(),
                        value: set_value_to_json(value),
                    },
                )?;
            }
        }
        // Phase B — deletes, reverse-sorted (numeric-aware desc) so
        // array-index removes don't shift earlier indices.
        let mut delete_paths: Vec<Vec<String>> = ops
            .iter()
            .filter_map(|op| match op {
                BulkOp::Delete { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        delete_paths.sort_by(|a, b| cmp_path_desc(a, b));
        delete_paths.dedup();
        for path in delete_paths {
            current = self.fmt.mutate(&current, SimpleMutation::RemoveAt { path })?;
        }
        Ok(current)
    }

    fn node_kind(&self, v: &Value) -> String {
        self.fmt.node_kind(v)
    }

    fn preview_for(&self, v: &Value) -> String {
        self.fmt.preview_for(v)
    }

    fn coerce_set_value(
        &self,
        _target_kind: &str,
        raw:          &crate::edit_expr::Value,
    ) -> Result<CoerceOutcome, CoerceSkip> {
        use arbor_studio_types::prelude::NullPolicy;
        use crate::edit_expr::Value as ExprValue;
        Ok(match raw {
            ExprValue::Null => {
                // FROZEN F13: AsDelete (TOML) routes null → delete;
                // Native (YAML) keeps it; AskUser / others keep a null
                // value too (the FE drove the decision before apply).
                match self.fmt.descriptor().null_handling {
                    NullPolicy::AsDelete => CoerceOutcome::DeleteInstead,
                    _                    => CoerceOutcome::Set(SetValue::Null),
                }
            }
            ExprValue::Bool(b)   => CoerceOutcome::Set(SetValue::Bool(*b)),
            ExprValue::Number(n) => CoerceOutcome::Set(refactor::coerce_number_default(*n)),
            ExprValue::String(s) => CoerceOutcome::Set(SetValue::String(s.clone())),
        })
    }
}

/// Numeric-aware descending path comparison for delete ordering.
fn cmp_path_desc(a: &[String], b: &[String]) -> std::cmp::Ordering {
    b.cmp(a)
}

/// Lower a `SetValue` to a `serde_json::Value` for `F::mutate`. Null is
/// only reachable for `Native` null policy (AsDelete routes to Delete).
fn set_value_to_json(v: &SetValue) -> Value {
    match v {
        SetValue::Null      => Value::Null,
        SetValue::Bool(b)   => Value::Bool(*b),
        SetValue::Int(i)    => Value::Number((*i).into()),
        SetValue::Float(f)  => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        SetValue::String(s) => Value::String(s.clone()),
    }
}

// ─── StudioFormatBackend ──────────────────────────────────────────────

#[async_trait]
impl<F: SimpleFormat> StudioFormatBackend for DefaultBackend<F> {
    fn descriptor(&self) -> &arbor_studio_types::prelude::FormatDescriptor {
        self.fmt.descriptor()
    }

    // ── Lifecycle ────────────────────────────────────────────────────

    async fn parse(
        &self,
        text:        String,
        source_path: Option<String>,
        encoding:    EncodingInfo,
    ) -> StudioResult<ParseResult> {
        let size_bytes  = text.len();
        let outcome     = self.fmt.parse(&text, &encoding);
        let root_kind   = self.root_kind_of(outcome.value.as_ref());
        let child_count = Self::child_count_of(outcome.value.as_ref());
        let indent      = self.fmt.detect_indent(&text);
        let parse_error = outcome.error.clone();
        let doc_id      = Uuid::new_v4().to_string();

        let state = DocState {
            original:       text.clone(),
            current:        text.clone(),
            value:          outcome.value,
            parse_error:    outcome.error,
            indent,
            source_path:    source_path.clone(),
            encoding_label: encoding.label.clone(),
            had_bom:        encoding.had_bom,
            history:        self.make_history(text),
        };
        self.lock()?.insert(doc_id.clone(), state);

        Ok(ParseResult {
            doc_id,
            size_bytes,
            source_path,
            original:    String::new(),
            parse_error,
            root_kind,
            child_count,
            schema_hint: None,
            encoding,
            stream_mode:        false,
            is_jsonc:           false,
            has_jsonc_features: false,
        })
    }

    fn close(&self, doc_id: &str) -> StudioResult<()> {
        self.lock()?.remove(doc_id);
        Ok(())
    }

    fn get_encoding(&self, doc_id: &str) -> StudioResult<EncodingInfo> {
        self.with_doc(doc_id, |d| Ok(EncodingInfo {
            label:   d.encoding_label.clone(),
            had_bom: d.had_bom,
        }))
    }

    // ── Text & raw access ────────────────────────────────────────────

    fn set_text(&self, doc_id: &str, text: String) -> StudioResult<UpdateResult> {
        self.with_doc_mut(doc_id, |doc| {
            doc.history.record_text(text.clone());
            let (root_kind, child_count) = self.refresh_projection(doc, text);
            Ok(UpdateResult {
                parse_error:        doc.parse_error.clone(),
                root_kind,
                child_count,
                can_undo:           doc.history.can_undo(),
                can_redo:           doc.history.can_redo(),
                has_jsonc_features: false,
            })
        })
    }

    fn raw_original(&self, doc_id: &str) -> StudioResult<String> {
        self.with_doc(doc_id, |d| Ok(d.original.clone()))
    }
    fn raw_current(&self, doc_id: &str) -> StudioResult<String> {
        self.with_doc(doc_id, |d| Ok(d.current.clone()))
    }
    fn format_doc(&self, doc_id: &str) -> StudioResult<String> {
        let text = self.with_doc(doc_id, |d| Ok(d.current.clone()))?;
        self.fmt.pretty(&text)
    }
    fn get_indent(&self, doc_id: &str) -> StudioResult<String> {
        self.with_doc(doc_id, |d| Ok(d.indent.clone()))
    }
    fn set_indent(&self, doc_id: &str, indent: String) -> StudioResult<()> {
        self.with_doc_mut(doc_id, |d| { d.indent = indent; Ok(()) })
    }

    // ── Tree navigation ──────────────────────────────────────────────

    fn get_root(&self, doc_id: &str) -> StudioResult<Option<NodeView>> {
        self.with_doc(doc_id, |doc| {
            Ok(doc.value.as_ref().map(|v| NodeView {
                key:         "$".to_string(),
                path:        Vec::new(),
                kind:        self.fmt.node_kind(v),
                preview:     self.fmt.preview_for(v),
                child_count: Self::child_count_of(Some(v)),
                variant_tag: self.fmt.variant_tag(v),
            }))
        })
    }

    fn get_children(&self, doc_id: &str, path: Vec<String>) -> StudioResult<Vec<NodeView>> {
        self.with_doc(doc_id, |doc| {
            let root = doc.value.as_ref().ok_or_else(|| {
                StudioError::App("Document has parse errors — cannot navigate".into())
            })?;
            let node = Self::resolve(root, &path)
                .ok_or_else(|| StudioError::App(format!("Missing path: {path:?}")))?;
            Ok(self.children_of(&path, node))
        })
    }

    fn get_value(&self, doc_id: &str, path: Vec<String>) -> StudioResult<String> {
        self.with_doc(doc_id, |doc| {
            let root = doc.value.as_ref().ok_or_else(|| {
                StudioError::App("Document has parse errors — cannot read value".into())
            })?;
            let node = Self::resolve(root, &path)
                .ok_or_else(|| StudioError::App(format!("Missing path: {path:?}")))?;
            serde_json::to_string_pretty(node).map_err(|e| StudioError::App(e.to_string()))
        })
    }

    // ── Query ────────────────────────────────────────────────────────

    fn query(&self, doc_id: &str, expr: String) -> StudioResult<Vec<QueryHit>> {
        self.with_doc(doc_id, |doc| {
            let root = doc.value.as_ref().ok_or_else(|| {
                StudioError::App("Document has parse errors — cannot query".into())
            })?;
            let locs = query::run(root, &expr, QUERY_MAX_HITS)
                .map_err(|e| StudioError::App(e.0))?;
            Ok(locs
                .into_iter()
                .map(|loc| QueryHit {
                    kind:        self.fmt.node_kind(&loc.value),
                    preview:     self.fmt.preview_for(&loc.value),
                    variant_tag: self.fmt.variant_tag(&loc.value),
                    path:        loc.path,
                })
                .collect())
        })
    }

    // ── Mutations ────────────────────────────────────────────────────

    fn apply_mutation(
        &self,
        doc_id:   &str,
        mutation: StudioMutation,
    ) -> StudioResult<MutateResult> {
        let simple = match mutation {
            StudioMutation::SetPrimitive { path, value } => SimpleMutation::SetPrimitive { path, value },
            StudioMutation::ReplaceAt { path, text }     => SimpleMutation::ReplaceAt { path, text },
            StudioMutation::RemoveAt { path }            => SimpleMutation::RemoveAt { path },
            StudioMutation::InsertField { path, name, text } => {
                SimpleMutation::InsertField { path, name, text }
            }
            StudioMutation::InsertItem { path, text } => SimpleMutation::InsertItem { path, text },
            StudioMutation::InsertMapEntry { path, key_text, val_text } => {
                SimpleMutation::InsertMapEntry { path, key_text, val_text }
            }
            StudioMutation::DuplicateAt { path } => SimpleMutation::DuplicateAt { path },
            StudioMutation::MoveItem { path, delta } => SimpleMutation::MoveItem { path, delta },
            // No simple format has Option/None — the descriptor declares
            // `null_handling != Native` toggle off, the FE never offers it.
            StudioMutation::ToggleOption { .. } => {
                return Err(StudioError::unsupported(self.fmt_id(), "toggle_option"));
            }
        };

        // Apply against the current text, then record a structural undo
        // step + refresh the projection.
        let text = self.with_doc(doc_id, |d| Ok(d.current.clone()))?;
        let new_text = self.fmt.mutate(&text, simple)?;
        self.with_doc_mut(doc_id, |doc| {
            doc.history.record_struct(new_text.clone());
            let (rk, cc) = self.refresh_projection(doc, new_text);
            Ok(Self::mutate_result(doc, rk, cc))
        })
    }

    // ── Diff ─────────────────────────────────────────────────────────

    fn diff(&self, doc_id: &str) -> StudioResult<Vec<DiffHunk>> {
        self.with_doc(doc_id, |d| Ok(diff::unified(&d.original, &d.current)))
    }

    fn tree_diff(&self, doc_id: &str) -> StudioResult<DiffTreeNode> {
        self.with_doc(doc_id, |doc| {
            // Project the original buffer fresh; the current projection is
            // already cached.
            let orig = self.fmt.parse(&doc.original, &EncodingInfo::utf8()).value;
            Ok(diff::tree_opt(orig.as_ref(), doc.value.as_ref()))
        })
    }

    // ── History ──────────────────────────────────────────────────────

    fn undo(&self, doc_id: &str) -> StudioResult<MutateResult> {
        self.with_doc_mut(doc_id, |doc| {
            let text = doc.history.undo()
                .ok_or_else(|| StudioError::App("Nothing to undo".into()))?
                .clone();
            let (rk, cc) = self.refresh_projection(doc, text);
            Ok(Self::mutate_result(doc, rk, cc))
        })
    }
    fn redo(&self, doc_id: &str) -> StudioResult<MutateResult> {
        self.with_doc_mut(doc_id, |doc| {
            let text = doc.history.redo()
                .ok_or_else(|| StudioError::App("Nothing to redo".into()))?
                .clone();
            let (rk, cc) = self.refresh_projection(doc, text);
            Ok(Self::mutate_result(doc, rk, cc))
        })
    }
    fn history_state(&self, doc_id: &str) -> StudioResult<(bool, bool)> {
        self.with_doc(doc_id, |d| Ok((d.history.can_undo(), d.history.can_redo())))
    }

    // ── Snapshot & persistence ───────────────────────────────────────

    fn snapshot(&self, doc_id: &str) -> StudioResult<DocSnapshot> {
        self.with_doc(doc_id, |doc| {
            let root_kind = self.root_kind_of(doc.value.as_ref());
            Ok(DocSnapshot {
                doc_id:      doc_id.to_string(),
                source_path: doc.source_path.clone(),
                size_bytes:  doc.current.len(),
                original:    doc.original.clone(),
                current:     doc.current.clone(),
                parse_error: doc.parse_error.clone(),
                root_kind,
                child_count: Self::child_count_of(doc.value.as_ref()),
                can_undo:    doc.history.can_undo(),
                can_redo:    doc.history.can_redo(),
                indent:      doc.indent.clone(),
            })
        })
    }

    fn source_path(&self, doc_id: &str) -> StudioResult<Option<String>> {
        self.with_doc(doc_id, |d| Ok(d.source_path.clone()))
    }

    async fn save(
        &self,
        doc_id:      &str,
        path:        String,
        contents:    String,
        bind_to_doc: bool,
    ) -> StudioResult<()> {
        // FROZEN F16 — round-trip the per-doc encoding through save.
        let (label, had_bom) =
            self.with_doc(doc_id, |d| Ok((d.encoding_label.clone(), d.had_bom)))?;
        crate::persist::write_encoded(&path, &contents, &label, had_bom)?;
        self.with_doc_mut(doc_id, |doc| {
            if bind_to_doc {
                doc.source_path = Some(path);
            }
            doc.original = doc.current.clone();
            Ok(())
        })
    }

    // ── File listing ─────────────────────────────────────────────────
    //
    // The repo scan lives in `arbor-studio-api` (Stage 4); `core` has no
    // handle on `scan_repo` / `StudioFileKind`. Until a format crate
    // overrides this, listing is unsupported here. (The launcher's
    // current backends override `list_files`; once they move onto
    // `DefaultBackend` the api layer supplies the scan.)
    async fn list_files(&self, folder: String) -> StudioResult<Vec<FileEntry>> {
        let _ = folder;
        Err(StudioError::unsupported(self.fmt_id(), "list_files"))
    }

    // ── Schema (routed to injected providers) ────────────────────────

    async fn schema_probe(&self, source: String) -> StudioResult<CrateProbe> {
        match self.schema.provider_for(&source) {
            Some(p) => p.probe(&source).await,
            None    => Err(StudioError::unsupported(self.fmt_id(), "schema_probe")),
        }
    }
    async fn schema_load(
        &self,
        source:         String,
        root_canonical: String,
    ) -> StudioResult<Schema> {
        match self.schema.provider_for(&source) {
            Some(p) => p.load(&source, &root_canonical).await,
            None    => Err(StudioError::unsupported(self.fmt_id(), "schema_load")),
        }
    }
    async fn schema_view_source(
        &self,
        source:         String,
        canonical_path: String,
    ) -> StudioResult<TypeSource> {
        match self.schema.provider_for(&source) {
            Some(p) => p.view_source(&source, &canonical_path).await,
            None    => Err(StudioError::unsupported(self.fmt_id(), "schema_view_source")),
        }
    }

    // ── F12 — Rename refactor ────────────────────────────────────────
    //
    // Index aggregation + repo scan live at the api/launcher call site
    // (core must not name `StudioIndex`/`scan_repo`). `DefaultBackend`
    // therefore can't run the project-wide preview by itself — that wiring
    // belongs to the api layer, which builds the def/usage slices and
    // calls `core::refactor::{build_rename_sites, rename_apply_files}`
    // with this backend as the `RefactorOps`. We expose the apply path
    // (FS-only, no index) and leave `rename_preview` to the api layer.

    async fn rename_apply(
        &self,
        _repo_root: String,
        old_value:  String,
        new_value:  String,
        sites:      Vec<RenameSite>,
        open_docs:  Vec<RenameOpenDoc>,
    ) -> StudioResult<RenameResult> {
        let affected = refactor::affected_path_set(
            sites.iter().map(|s| s.absolute_path.as_str()),
        );
        if refactor::any_affected_dirty(&rename_open_doc_states(&open_docs), &affected) {
            return Err(StudioError::App(
                "Some affected files have unsaved changes. Save or discard first.".into(),
            ));
        }
        refactor::rename_apply_files(self, &old_value, &new_value, &sites)
    }

    // `rename_preview` stays the trait default (Unsupported) on
    // DefaultBackend itself — the api layer provides the index-aware
    // preview. (Stage 4 wires this; Stage 3 only proves the generic.)

    // ── F13 — Query-driven bulk edit ─────────────────────────────────

    async fn bulk_edit_preview(
        &self,
        _repo_root:   String,
        doc_id:       String,
        scope:        BulkEditScope,
        query:        String,
        action:       BulkEditAction,
        value_source: Option<BulkEditValueSource>,
        _open_docs:   Vec<BulkEditOpenDoc>,
    ) -> StudioResult<BulkEditPreview> {
        let compiled = match refactor::compile_expression(action, &value_source) {
            Ok(c)  => c,
            Err(e) => return Ok(BulkEditPreview {
                sites:            Vec::new(),
                dirty_blockers:   Vec::new(),
                expression_error: Some(e),
            }),
        };
        match scope {
            BulkEditScope::ActiveDoc => {
                let (source_path, pairs) = self.with_doc(&doc_id, |doc| {
                    let source_path = doc.source_path.clone();
                    let root = doc.value.as_ref().ok_or_else(|| {
                        StudioError::App("Document has parse errors — cannot query".into())
                    })?;
                    let locs = query::run(root, &query, QUERY_MAX_HITS)
                        .map_err(|e| StudioError::App(e.0))?;
                    let pairs: Vec<(Vec<String>, Value)> =
                        locs.into_iter().map(|l| (l.path, l.value)).collect();
                    Ok((source_path, pairs))
                })?;
                let sites = refactor::build_active_doc_sites(
                    self, &source_path, pairs, action, &value_source, compiled.as_ref(),
                );
                Ok(BulkEditPreview { sites, dirty_blockers: Vec::new(), expression_error: None })
            }
            // Project-wide preview needs the repo scan (api/launcher).
            BulkEditScope::ProjectWide => {
                Err(StudioError::unsupported(self.fmt_id(), "bulk_edit_preview(project_wide)"))
            }
        }
    }

    async fn bulk_edit_apply(
        &self,
        _repo_root:   String,
        doc_id:       String,
        scope:        BulkEditScope,
        action:       BulkEditAction,
        value_source: Option<BulkEditValueSource>,
        sites:        Vec<BulkEditSite>,
        open_docs:    Vec<BulkEditOpenDoc>,
    ) -> StudioResult<BulkEditResult> {
        let compiled = refactor::compile_expression(action, &value_source)
            .map_err(|e| StudioError::App(format!("Expression compile error: {e}")))?;
        match scope {
            BulkEditScope::ActiveDoc => {
                let (state, applied, skipped) = self.bulk_apply_active(
                    &doc_id, &sites, action, &value_source, compiled.as_ref(),
                )?;
                Ok(BulkEditResult {
                    written_files:    Vec::new(),
                    failed_files:     Vec::new(),
                    applied_sites:    applied,
                    skipped_sites:    skipped,
                    active_doc_state: state,
                })
            }
            BulkEditScope::ProjectWide => {
                let affected = refactor::affected_path_set(
                    sites.iter().map(|s| s.absolute_path.as_str()),
                );
                if refactor::any_affected_dirty(&bulk_open_doc_states(&open_docs), &affected) {
                    return Err(StudioError::App(
                        "Some affected files have unsaved changes. Save or discard first.".into(),
                    ));
                }
                let id = self.fmt_id();
                refactor::bulk_apply_files(
                    self, sites, action, &value_source, compiled.as_ref(),
                    move |p| format!("parse {p}: invalid {id}"),
                )
            }
        }
    }
}

impl<F: SimpleFormat> DefaultBackend<F> {
    /// Build the child `NodeView`s of a container value at `parent_path`.
    fn children_of(&self, parent_path: &[String], v: &Value) -> Vec<NodeView> {
        let make = |key: String, child: &Value| {
            let mut path = parent_path.to_vec();
            path.push(key.clone());
            NodeView {
                key,
                kind:        self.fmt.node_kind(child),
                preview:     self.fmt.preview_for(child),
                child_count: Self::child_count_of(Some(child)),
                variant_tag: self.fmt.variant_tag(child),
                path,
            }
        };
        match v {
            Value::Object(m) => m.iter().map(|(k, c)| make(k.clone(), c)).collect(),
            Value::Array(a)  => a.iter().enumerate().map(|(i, c)| make(i.to_string(), c)).collect(),
            _ => Vec::new(),
        }
    }
}

/// Lower the FE's `RenameOpenDoc` list to the engine's `OpenDocState`.
fn rename_open_doc_states(docs: &[RenameOpenDoc]) -> Vec<OpenDocState> {
    docs.iter()
        .map(|d| OpenDocState {
            doc_id:      d.doc_id.clone(),
            source_path: d.source_path.clone(),
            dirty:       d.dirty,
        })
        .collect()
}

/// Lower the FE's `BulkEditOpenDoc` list to the engine's `OpenDocState`.
fn bulk_open_doc_states(docs: &[BulkEditOpenDoc]) -> Vec<OpenDocState> {
    docs.iter()
        .map(|d| OpenDocState {
            doc_id:      d.doc_id.clone(),
            source_path: d.source_path.clone(),
            dirty:       d.dirty,
        })
        .collect()
}

#[cfg(test)]
mod tests;
