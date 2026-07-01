//! `JsonBackend` — `StudioFormatBackend` implementation for JSON.
//!
//! A position-preserving editor: mutations splice bytes at AST node
//! ranges (`ast.rs` + `edits.rs`), `set_text` accepts raw textarea
//! input with history coalescing, undo/redo walks a snapshot stack,
//! diff renders unified hunks vs the original buffer, save round-trips
//! through `arbor_fs::encoding::encode_for_disk_with_bom` (FROZEN F16).
//!
//! JSON is a hand-written special (NOT `DefaultBackend`): dual parser
//! (`simd-json` read path + `jsonc-parser` byte-splice edit path),
//! sticky per-doc stream mode for multi-MB files, JSONC comments /
//! trailing commas, and an AST tree-diff that distinguishes `1.0` from
//! `1.00` via `Number.raw`. It still calls `arbor-studio-core`'s engines
//! (history / diff / query / edit_expr / refactor / persist).
//!
//! State (the `JsonStudioRegistry` map) lives behind a `Mutex` inside
//! the backend so the trait can expose `&self` methods.
//!
//! F12 / F13 are **self-serving**: the backend runs its own project-wide
//! `rename_preview` / `bulk_edit_preview` against the caller-supplied
//! [`JsonIndexProvider`] (the repo scanner + cross-ref index live in the
//! launcher / `arbor-studio-api`, which the crate must not name).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::registry::{self as legacy, DocParseMode, JsonStudioRegistry, NodeKind};
use crate::bulk_edits::{JsonBulkOp, JsonSetValue};
use crate::index_provider::{NoIndexProvider, SharedIndexProvider};

/// FROZEN F14 default: 1 MB. Above this size we open in stream mode
/// (simd_json strict, navigation-only). The descriptor surfaces this
/// as `streaming_threshold_kb = Some(1024)` so the FE can mirror the
/// number in tooltips / plugin settings without re-asking the BE.
const STREAM_THRESHOLD_BYTES: usize = 1024 * 1024;

use arbor_studio_core::prelude::edit_expr::Value as ExprValue;
use arbor_studio_core::prelude::{
    self as core, refactor, BulkOp, CoerceOutcome, CoerceSkip, OpenDocState, RefactorOps,
    SetValue, StudioFormatBackend,
};
use arbor_studio_types::prelude::{
    BulkEditAction, BulkEditOpenDoc, BulkEditPreview, BulkEditResult, BulkEditScope,
    BulkEditSite, BulkEditValueSource, CrateProbe, CrossRefScope, DiffHunk, DiffTreeNode,
    DocSnapshot, EncodingInfo, FileEntry, FormatDescriptor, IconRef, KindStyle, KindTone,
    MutateResult, NodeView, NullPolicy, ParseResult, QueryHit, QuerySyntax,
    RenameOpenDoc, RenamePreview, RenameResult, RenameSite, SaveWarningKind, Schema,
    SchemaSourceKind, StudioError, StudioMutation, StudioResult, TypeSource, UpdateResult,
};

pub struct JsonBackend {
    regs:       Mutex<JsonStudioRegistry>,
    descriptor: FormatDescriptor,
    index:      SharedIndexProvider,
}

impl JsonBackend {
    /// Build a JSON backend with the given index/scanner provider. The
    /// launcher injects one wrapping its repo scanner + cross-ref index;
    /// pass [`NoIndexProvider`] for the active-doc-only / test path.
    pub fn with_index(index: SharedIndexProvider) -> Self {
        Self {
            regs:       Mutex::new(JsonStudioRegistry::default()),
            descriptor: build_descriptor(),
            index,
        }
    }

    pub fn new() -> Self {
        Self::with_index(Arc::new(NoIndexProvider))
    }

    fn lock(&self) -> StudioResult<std::sync::MutexGuard<'_, JsonStudioRegistry>> {
        self.regs
            .lock()
            .map_err(|_| StudioError::App("json_studio registry poisoned".into()))
    }
}

impl Default for JsonBackend {
    fn default() -> Self { Self::new() }
}

/// Public factory — JSON backend with no index provider (active-doc only).
/// The launcher uses [`backend_with_index`] to wire the repo scanner.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    Arc::new(JsonBackend::new())
}

/// Public factory — JSON backend with the caller's index/scanner provider
/// wired for project-wide F12/F13.
pub fn backend_with_index(index: SharedIndexProvider) -> Arc<dyn StudioFormatBackend> {
    Arc::new(JsonBackend::with_index(index))
}

#[async_trait]
impl StudioFormatBackend for JsonBackend {
    fn descriptor(&self) -> &FormatDescriptor { &self.descriptor }

    // ── Lifecycle ────────────────────────────────────────────────────

    async fn parse(
        &self,
        text:        String,
        source_path: Option<String>,
        encoding:    EncodingInfo,
    ) -> StudioResult<ParseResult> {
        let legacy_res = self.lock()?.parse(
            text,
            source_path,
            encoding.label.clone(),
            encoding.had_bom,
            STREAM_THRESHOLD_BYTES,
        );
        Ok(parse_result_into(legacy_res, encoding))
    }

    fn close(&self, doc_id: &str) -> StudioResult<()> {
        self.lock()?.close(doc_id);
        Ok(())
    }

    fn get_encoding(&self, doc_id: &str) -> StudioResult<EncodingInfo> {
        let (label, had_bom) = self.lock()?.encoding_info(doc_id)?;
        Ok(EncodingInfo { label, had_bom })
    }

    // ── Text & raw access ────────────────────────────────────────────

    fn set_text(&self, doc_id: &str, text: String) -> StudioResult<UpdateResult> {
        let res = self.lock()?.set_text(doc_id, text)?;
        Ok(update_result_into(res))
    }

    fn raw_original(&self, doc_id: &str) -> StudioResult<String> {
        self.lock()?.raw_original(doc_id)
    }

    fn raw_current(&self, doc_id: &str) -> StudioResult<String> {
        self.lock()?.raw_current(doc_id)
    }

    fn format_doc(&self, doc_id: &str) -> StudioResult<String> {
        self.lock()?.pretty(doc_id)
    }

    fn get_indent(&self, doc_id: &str) -> StudioResult<String> {
        self.lock()?.get_indent(doc_id)
    }

    fn set_indent(&self, doc_id: &str, indent: String) -> StudioResult<()> {
        self.lock()?.set_indent(doc_id, indent)
    }

    // ── Tree navigation ──────────────────────────────────────────────

    fn get_root(&self, doc_id: &str) -> StudioResult<Option<NodeView>> {
        // Returns None when the doc is unparseable so the FE can show
        // a parse-error placeholder instead of an empty tree.
        match self.lock()?.get_root(doc_id) {
            Ok(v) => Ok(Some(node_view_into(v))),
            Err(_) => Ok(None),
        }
    }

    fn get_children(
        &self,
        doc_id: &str,
        path:   Vec<String>,
    ) -> StudioResult<Vec<NodeView>> {
        Ok(self
            .lock()?
            .get_children(doc_id, &path)?
            .into_iter()
            .map(node_view_into)
            .collect())
    }

    fn get_value(&self, doc_id: &str, path: Vec<String>) -> StudioResult<String> {
        self.lock()?.get_value_pretty(doc_id, &path)
    }

    // ── Query ────────────────────────────────────────────────────────

    fn query(&self, doc_id: &str, expr: String) -> StudioResult<Vec<QueryHit>> {
        Ok(self
            .lock()?
            .query(doc_id, &expr)?
            .into_iter()
            .map(query_hit_into)
            .collect())
    }

    // ── Mutations ────────────────────────────────────────────────────

    fn apply_mutation(
        &self,
        doc_id:   &str,
        mutation: StudioMutation,
    ) -> StudioResult<MutateResult> {
        let mut reg = self.lock()?;
        let res = match mutation {
            StudioMutation::SetPrimitive { path, value } => {
                reg.mutate_primitive(doc_id, &path, value)?
            }
            StudioMutation::ReplaceAt { path, text } => {
                reg.replace_at(doc_id, &path, text)?
            }
            StudioMutation::RemoveAt { path } => reg.remove_at(doc_id, &path)?,
            StudioMutation::InsertField { path, name, text } => {
                reg.insert_field(doc_id, &path, name, text)?
            }
            StudioMutation::InsertItem { path, text } => {
                reg.insert_item(doc_id, &path, text)?
            }
            StudioMutation::InsertMapEntry { path, key_text, val_text } => {
                reg.insert_map_entry(doc_id, &path, key_text, val_text)?
            }
            StudioMutation::DuplicateAt { path } => reg.duplicate_at(doc_id, &path)?,
            StudioMutation::MoveItem { path, delta } => reg.move_item(doc_id, &path, delta)?,
            // JSON has no Option/None — toggling option on a JSON node
            // is undefined. Gate-off via descriptor (`null_handling =
            // Native` means delete-or-keep is what the FE offers).
            StudioMutation::ToggleOption { .. } => {
                return Err(StudioError::unsupported("json", "toggle_option"));
            }
        };
        Ok(mutate_result_into(res))
    }

    // ── Diff ─────────────────────────────────────────────────────────

    fn diff(&self, doc_id: &str) -> StudioResult<Vec<DiffHunk>> {
        self.lock()?.diff(doc_id)
    }

    fn tree_diff(&self, doc_id: &str) -> StudioResult<DiffTreeNode> {
        self.lock()?.tree_diff(doc_id)
    }

    // ── Strip JSONC features ─────────────────────────────────────────

    fn strip_features(&self, doc_id: &str) -> StudioResult<MutateResult> {
        Ok(mutate_result_into(self.lock()?.strip_jsonc_features(doc_id)?))
    }

    // ── History ──────────────────────────────────────────────────────

    fn undo(&self, doc_id: &str) -> StudioResult<MutateResult> {
        Ok(mutate_result_into(self.lock()?.undo(doc_id)?))
    }

    fn redo(&self, doc_id: &str) -> StudioResult<MutateResult> {
        Ok(mutate_result_into(self.lock()?.redo(doc_id)?))
    }

    fn history_state(&self, doc_id: &str) -> StudioResult<(bool, bool)> {
        self.lock()?.history_state(doc_id)
    }

    // ── Snapshot & persistence ───────────────────────────────────────

    fn snapshot(&self, doc_id: &str) -> StudioResult<DocSnapshot> {
        let reg = self.lock()?;
        let original    = reg.raw_original(doc_id)?;
        let current     = reg.raw_current(doc_id)?;
        let source_path = reg.source_path(doc_id)?;
        let parse_error = reg.parse_error(doc_id)?;
        let root_kind   = reg.root_kind(doc_id)?.map(kind_to_string);
        let child_count = reg.root_child_count(doc_id)?;
        let indent      = reg.get_indent(doc_id)?;
        let (can_undo, can_redo) = reg.history_state(doc_id)?;
        let size_bytes  = current.len();
        Ok(DocSnapshot {
            doc_id:      doc_id.to_string(),
            source_path,
            size_bytes,
            original,
            current,
            parse_error,
            root_kind,
            child_count,
            can_undo,
            can_redo,
            indent,
        })
    }

    fn source_path(&self, doc_id: &str) -> StudioResult<Option<String>> {
        self.lock()?.source_path(doc_id)
    }

    async fn save(
        &self,
        doc_id:      &str,
        path:        String,
        contents:    String,
        bind_to_doc: bool,
    ) -> StudioResult<()> {
        // FROZEN F16: look up the per-doc encoding so windows-1252 /
        // UTF-16 BOM files round-trip without corruption. Save-As to a
        // different path preserves the source encoding by design.
        let (encoding_label, had_bom) = self.lock()?.encoding_info(doc_id)?;
        legacy::write_to_disk(&path, &contents, &encoding_label, had_bom)?;
        let mut reg = self.lock()?;
        if bind_to_doc {
            reg.rebind_source(doc_id, path)?;
        }
        reg.mark_saved(doc_id)?;
        Ok(())
    }

    // ── File listing ─────────────────────────────────────────────────

    async fn list_files(&self, folder: String) -> StudioResult<Vec<FileEntry>> {
        // JSON files come from the caller-supplied scanner (JSON slice).
        let index = self.index.clone();
        let entries = tokio::task::spawn_blocking(move || index.scan_files(&folder))
            .await
            .map_err(|e| StudioError::App(format!("list_files join: {e}")))??;
        Ok(entries
            .into_iter()
            .map(|e| FileEntry {
                absolute_path: e.absolute_path,
                relative_path: e.relative_path,
                name:          e.name,
                size_bytes:    e.size_bytes,
            })
            .collect())
    }

    // ── Convert ──────────────────────────────────────────────────────

    fn to_json(&self, doc_id: &str) -> StudioResult<String> {
        // JSON-to-JSON: hand back the live edited buffer verbatim.
        self.lock()?.raw_current(doc_id)
    }

    // ── F12 — Rename refactor (lossless byte splice) ─────────────────

    async fn rename_preview(
        &self,
        repo_root:      String,
        old_value:      String,
        new_value_hint: Option<String>,
        open_docs:      Vec<RenameOpenDoc>,
    ) -> StudioResult<RenamePreview> {
        if old_value.is_empty() {
            return Err(StudioError::App("Rename target value is empty".into()));
        }
        let index = self.index.clone();
        let preview = tokio::task::spawn_blocking(move || -> StudioResult<RenamePreview> {
            let (defs, usages) = index.rename_inputs(&repo_root, &old_value)?;

            let mut sites = refactor::build_rename_sites(
                &defs, &usages, &old_value, refactor::DefScopeStyle::Definition,
            );
            // Best-effort line-snippet preview (JSON-specific heuristic).
            let mut file_text_cache: BTreeMap<String, String> = BTreeMap::new();
            for site in sites.iter_mut() {
                let text = file_text_cache
                    .entry(site.absolute_path.clone())
                    .or_insert_with(|| read_file_to_string(&site.absolute_path));
                site.preview = synth_preview_line(text, &site.key_name, &old_value);
            }
            let collisions = refactor::collisions_for(&defs, new_value_hint.as_deref(), &old_value);
            let affected   = refactor::affected_path_set(
                sites.iter().map(|s| s.absolute_path.as_str()),
            );
            let dirty_blockers = refactor::dirty_blockers_for(
                &rename_open_doc_states(open_docs), &affected,
            );

            Ok(RenamePreview { sites, dirty_blockers, collisions })
        })
        .await
        .map_err(|e| StudioError::App(format!("rename_preview join: {e}")))??;
        Ok(preview)
    }

    async fn rename_apply(
        &self,
        repo_root: String,
        old_value: String,
        new_value: String,
        sites:     Vec<RenameSite>,
        open_docs: Vec<RenameOpenDoc>,
    ) -> StudioResult<RenameResult> {
        if new_value.is_empty() {
            return Err(StudioError::App("New value is empty".into()));
        }
        if new_value == old_value {
            return Err(StudioError::App(
                "New value equals old value — nothing to rename".into(),
            ));
        }
        if sites.is_empty() {
            return Err(StudioError::App("No sites selected for rename".into()));
        }

        // Defensive apply-time dirty re-check (shared helper).
        let affected = refactor::affected_path_set(
            sites.iter().map(|s| s.absolute_path.as_str()),
        );
        if refactor::any_affected_dirty(&rename_open_doc_states(open_docs), &affected) {
            return Err(StudioError::App(
                "Some affected files have unsaved changes. Save or discard first.".into(),
            ));
        }
        let _ = repo_root;

        let result = tokio::task::spawn_blocking(move || -> StudioResult<RenameResult> {
            refactor::rename_apply_files(&JsonRefactor, &old_value, &new_value, &sites)
        })
        .await
        .map_err(|e| StudioError::App(format!("rename_apply join: {e}")))??;
        Ok(result)
    }

    // ── F13 — Query-driven bulk edit (lossless byte splice) ─────────

    async fn bulk_edit_preview(
        &self,
        repo_root:    String,
        doc_id:       String,
        scope:        BulkEditScope,
        query:        String,
        action:       BulkEditAction,
        value_source: Option<BulkEditValueSource>,
        open_docs:    Vec<BulkEditOpenDoc>,
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
                let pairs = self.lock()?.query_value_pairs(&doc_id, &query)?;
                let source_path = self.lock()?.source_path(&doc_id)?;
                let sites = refactor::build_active_doc_sites(
                    &JsonRefactor, &source_path, pairs, action, &value_source, compiled.as_ref(),
                );
                Ok(BulkEditPreview {
                    sites,
                    dirty_blockers:   Vec::new(),
                    expression_error: None,
                })
            }
            BulkEditScope::ProjectWide => {
                let query     = query.clone();
                let value_src = value_source.clone();
                let compiled  = compiled.clone();
                let index     = self.index.clone();
                tokio::task::spawn_blocking(move || -> StudioResult<BulkEditPreview> {
                    let mut sites: Vec<BulkEditSite> = Vec::new();
                    let files = index.scan_files(&repo_root)?;
                    for f in &files {
                        if f.excluded { continue; }
                        let text = core::persist::read_to_string_lossy(&f.absolute_path);
                        // Lenient parse via the JSON-specific RefactorOps leaf
                        // (keeps `.jsonc` comments / trailing commas safe).
                        let Some(root) = JsonRefactor.parse_to_value(&text) else { continue; };
                        let pairs = match legacy::query_value_pairs_against(&root, &query) {
                            Ok(p)  => p,
                            Err(_) => continue,
                        };
                        for (path, pair_value) in pairs {
                            sites.push(refactor::build_bulk_site(
                                &JsonRefactor,
                                &f.absolute_path,
                                &f.relative_path,
                                &f.name,
                                &path, &pair_value,
                                action, &value_src, compiled.as_ref(),
                            ));
                        }
                    }
                    sites.sort_by(|a, b|
                        a.relative_path.cmp(&b.relative_path)
                            .then_with(|| a.field_path.cmp(&b.field_path))
                    );

                    let affected = refactor::affected_path_set(
                        sites.iter().map(|s| s.absolute_path.as_str()),
                    );
                    let dirty_blockers = refactor::dirty_blockers_for(
                        &bulk_open_doc_states(open_docs), &affected,
                    );

                    Ok(BulkEditPreview {
                        sites,
                        dirty_blockers,
                        expression_error: None,
                    })
                })
                .await
                .map_err(|e| StudioError::App(format!("bulk_edit_preview join: {e}")))?
            }
        }
    }

    async fn bulk_edit_apply(
        &self,
        repo_root:    String,
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
                let (ops, applied, skipped) = {
                    let reg = self.lock()?;
                    let root_value = reg.query_value_pairs(&doc_id, "$")?
                        .into_iter().next()
                        .map(|(_p, v)| v)
                        .unwrap_or(serde_json::Value::Null);
                    refactor::build_bulk_ops(
                        &JsonRefactor, &root_value, &sites, action, &value_source, compiled.as_ref(),
                    )
                };
                let state = if ops.is_empty() {
                    None
                } else {
                    let json_ops = to_json_ops(&ops);
                    let mut reg = self.lock()?;
                    Some(mutate_result_into(reg.apply_bulk_edits_doc(&doc_id, &json_ops)?))
                };
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
                if refactor::any_affected_dirty(&bulk_open_doc_states(open_docs), &affected) {
                    return Err(StudioError::App(
                        "Some affected files have unsaved changes. Save or discard first.".into(),
                    ));
                }
                let _ = repo_root;

                let value_src = value_source.clone();
                let compiled  = compiled.clone();
                let result = tokio::task::spawn_blocking(move || -> StudioResult<BulkEditResult> {
                    refactor::bulk_apply_files(
                        &JsonRefactor, sites, action, &value_src, compiled.as_ref(),
                        |p| format!("parse {p}: invalid JSON"),
                    )
                })
                .await
                .map_err(|e| StudioError::App(format!("bulk_edit_apply join: {e}")))??;
                Ok(result)
            }
        }
    }

    // ── Schema (JSON Schema sidecar) — JSON serves its own ───────────

    async fn schema_probe(&self, source: String) -> StudioResult<CrateProbe> {
        let src = source.clone();
        tokio::task::spawn_blocking(move || crate::schema::probe(&src))
            .await
            .map_err(|e| StudioError::App(format!("schema_probe join: {e}")))?
    }

    async fn schema_load(
        &self,
        source:          String,
        root_canonical: String,
    ) -> StudioResult<Schema> {
        let src  = source.clone();
        let root = root_canonical.clone();
        tokio::task::spawn_blocking(move || crate::schema::load(&src, &root))
            .await
            .map_err(|e| StudioError::App(format!("schema_load join: {e}")))?
    }

    async fn schema_view_source(
        &self,
        source:         String,
        canonical_path: String,
    ) -> StudioResult<TypeSource> {
        let src       = source.clone();
        let canonical = canonical_path.clone();
        tokio::task::spawn_blocking(move || crate::schema::get_type_source(&src, &canonical))
            .await
            .map_err(|e| StudioError::App(format!("schema_view_source join: {e}")))?
    }
}

// ─── Open-doc dirty-state mapping (pure DTO → core shape) ─────────────

fn rename_open_doc_states(docs: Vec<RenameOpenDoc>) -> Vec<OpenDocState> {
    docs.into_iter()
        .map(|d| OpenDocState {
            doc_id:      d.doc_id,
            source_path: d.source_path,
            dirty:       d.dirty,
        })
        .collect()
}

fn bulk_open_doc_states(docs: Vec<BulkEditOpenDoc>) -> Vec<OpenDocState> {
    docs.into_iter()
        .map(|d| OpenDocState {
            doc_id:      d.doc_id,
            source_path: d.source_path,
            dirty:       d.dirty,
        })
        .collect()
}

// ─── RefactorOps — JSON leaf operations for core::refactor ───────────
//
// JSON edits are byte-splices over a lenient AST so `.jsonc` comments /
// trailing commas survive. `parse_to_value` therefore uses the lenient
// (`strict=false`) parse path the project-wide F12/F13 flows relied on.
// `null_handling = Native`: `null` is a first-class set value.

pub(crate) struct JsonRefactor;

impl RefactorOps for JsonRefactor {
    fn parse_to_value(&self, text: &str) -> Option<serde_json::Value> {
        crate::ast::parse_with(text, /* strict */ false)
            .ok()
            .map(|ast| crate::ast::ast_to_value(&ast))
    }

    fn apply_string_rename(
        &self,
        text:  &str,
        paths: &[Vec<String>],
        new:   &str,
    ) -> StudioResult<String> {
        legacy::apply_string_rename(text, paths, new)
    }

    fn apply_bulk_ops(&self, text: &str, ops: &[BulkOp]) -> StudioResult<String> {
        let json_ops = to_json_ops(ops);
        crate::bulk_edits::apply_bulk_edits_text(text, &json_ops)
    }

    fn node_kind(&self, v: &serde_json::Value) -> String {
        legacy::json_kind_str(v).to_string()
    }

    fn preview_for(&self, v: &serde_json::Value) -> String {
        legacy::json_preview_for(v)
    }

    fn coerce_set_value(
        &self,
        _target_kind: &str,
        raw:          &ExprValue,
    ) -> Result<CoerceOutcome, CoerceSkip> {
        Ok(match raw {
            ExprValue::Null      => CoerceOutcome::Set(SetValue::Null),
            ExprValue::Bool(b)   => CoerceOutcome::Set(SetValue::Bool(*b)),
            ExprValue::Number(n) => CoerceOutcome::Set(refactor::coerce_number_default(*n)),
            ExprValue::String(s) => CoerceOutcome::Set(SetValue::String(s.clone())),
        })
    }
}

/// Lower the format-agnostic `BulkOp` batch to JSON's `(path, JsonBulkOp)`
/// shape consumed by the byte-splice writer.
fn to_json_ops(ops: &[BulkOp]) -> Vec<(Vec<String>, JsonBulkOp)> {
    ops.iter()
        .map(|op| match op {
            BulkOp::Delete { path } => (path.clone(), JsonBulkOp::Delete),
            BulkOp::Set { path, value } => (path.clone(), JsonBulkOp::Set(set_value_to_json(value))),
        })
        .collect()
}

fn set_value_to_json(v: &SetValue) -> JsonSetValue {
    match v {
        SetValue::Null      => JsonSetValue::Null,
        SetValue::Bool(b)   => JsonSetValue::Bool(*b),
        SetValue::Int(i)    => JsonSetValue::Number(*i as f64),
        SetValue::Float(f)  => JsonSetValue::Number(*f),
        SetValue::String(s) => JsonSetValue::String(s.clone()),
    }
}

// ─── F12 helpers ─────────────────────────────────────────────────────

fn read_file_to_string(abs_path: &str) -> String {
    let Ok(bytes) = std::fs::read(abs_path) else { return String::new(); };
    let (text, _, _) = arbor_fs::prelude::encoding::decode_bytes_full(&bytes);
    text
}

/// JSON-aware preview line — looks for `"key": ... "value"` co-occurrence
/// on the same line, falls back to a value-only match for cases where
/// the key sits on a previous line. Trim to ~80 chars for the modal.
fn synth_preview_line(text: &str, key: &str, value: &str) -> String {
    if text.is_empty() { return String::new(); }
    let needle_key = format!("\"{key}\"");
    let needle_val = format!("\"{value}\"");
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() { continue; }
        if l.contains(&needle_key) && l.contains(&needle_val) {
            best = Some(l);
            break;
        }
        if best.is_none() && l.contains(&needle_val) {
            best = Some(l);
        }
    }
    let line = best.unwrap_or("").to_string();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

// ─── Descriptor ───────────────────────────────────────────────────────

fn build_descriptor() -> FormatDescriptor {
    let mut kind_palette = BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    kind_palette.insert("object".into(), entry("object", KindTone::Info));
    kind_palette.insert("array".into(),  entry("array",  KindTone::Info));
    kind_palette.insert("string".into(), entry("string", KindTone::Success));
    kind_palette.insert("number".into(), entry("number", KindTone::Warning));
    kind_palette.insert("bool".into(),   entry("bool",   KindTone::Warning));
    kind_palette.insert("null".into(),   entry("null",   KindTone::Muted));

    FormatDescriptor {
        id:                          "json".into(),
        label:                       "JSON".into(),
        // `.jsonc` joins the association. The plugin Lua picker also
        // includes the extension; the studio sidebar maps `jsonc` →
        // `StudioFileKind::Json` so the same backend handles both kinds.
        file_extensions:             vec![".json".into(), ".jsonc".into()],
        icon:                        IconRef::Iconify { name: "vscode-icons:file-type-json".into() },

        // Mutations splice AST byte ranges in tree mode; stream-mode docs
        // return `Unsupported` on mutations (large files, navigation-only).
        supports_lossless_edit:      true,
        // Comments + trailing commas accepted in tree mode (`jsonc-parser`
        // lenient). Strict `.json` files don't have them; `.jsonc` files
        // do. The flag advertises the capability; per-doc behaviour is
        // driven by `is_jsonc` + the file content.
        supports_comments:           true,
        supports_anchors:            false,
        null_handling:               NullPolicy::Native,

        // Stream mode (FROZEN F14): files ≥ 1 MB open in `simd_json`
        // strict mode, no AST, no structural editing. The descriptor's
        // threshold mirrors the BE constant so the FE can show the
        // expected breakpoint in plugin settings / tooltips.
        supports_streaming_mode:     true,
        streaming_threshold_kb:      Some((STREAM_THRESHOLD_BYTES / 1024) as u32),
        streaming_setting_key:       Some("json-studio.ast_threshold_kb".into()),

        query_syntax:                QuerySyntax::JsonPath,

        cross_ref_default_fields:    vec!["id".into(), "name".into()],
        cross_ref_scopes:            vec![CrossRefScope::Value],

        schema_sources:              vec![SchemaSourceKind::JsonSchema],

        kind_palette,

        // The catalogue is per-format; per-doc the FE checks
        // `parse_result.has_jsonc_features` + `is_jsonc` to know whether
        // to actually surface this banner.
        save_warnings:               vec![SaveWarningKind::JsoncCommentsInJson],
        save_behavior_setting_key:   Some("json-studio.save_behavior".into()),

        // `to_json` returns the live buffer — meaningful only when a
        // sibling format wants to ingest JSON. JSON's own button stays
        // hidden via this flag (no useful "convert JSON to JSON").
        convert_to_json_supported:   false,

        supports_external_files:     true,

        // F12 + F13: the backend implements `rename_preview` /
        // `rename_apply` and the `bulk_edit_*` pair against a JSON-only
        // slice of the studio index (cross-refs) plus byte-splice
        // mutations for the active doc / project-wide flows.
        supports_rename_reference:   true,
        supports_bulk_edit:          true,
    }
}

// ─── Type conversions ────────────────────────────────────────────────

fn kind_to_str(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Object => "object",
        NodeKind::Array  => "array",
        NodeKind::String => "string",
        NodeKind::Number => "number",
        NodeKind::Bool   => "bool",
        NodeKind::Null   => "null",
    }
}

fn kind_to_string(k: NodeKind) -> String { kind_to_str(k).to_string() }

fn node_view_into(v: legacy::NodeView) -> NodeView {
    NodeView {
        key:         v.key,
        path:        v.path,
        kind:        kind_to_string(v.kind),
        preview:     v.preview,
        child_count: v.child_count,
        variant_tag: None,
    }
}

fn query_hit_into(h: legacy::QueryHit) -> QueryHit {
    QueryHit {
        path:        h.path,
        kind:        kind_to_string(h.kind),
        preview:     h.preview,
        variant_tag: None,
    }
}

fn parse_result_into(p: legacy::ParseResult, encoding: EncodingInfo) -> ParseResult {
    ParseResult {
        doc_id:      p.doc_id,
        size_bytes:  p.size_bytes,
        source_path: p.source_path,
        // FE pushed the text (or the command layer just read it) so
        // round-tripping it through `original` is wasted bandwidth. The
        // modal calls `raw_original` lazily.
        original:    String::new(),
        parse_error: p.parse_error,
        root_kind:   p.root_kind.map(kind_to_string),
        child_count: p.child_count,
        schema_hint: None,
        encoding,
        stream_mode:        matches!(p.parse_mode, DocParseMode::Stream),
        is_jsonc:           p.is_jsonc,
        has_jsonc_features: p.has_jsonc_features,
    }
}

fn update_result_into(u: legacy::UpdateResult) -> UpdateResult {
    UpdateResult {
        parse_error:        u.parse_error,
        root_kind:          u.root_kind.map(kind_to_string),
        child_count:        u.child_count,
        can_undo:           u.can_undo,
        can_redo:           u.can_redo,
        has_jsonc_features: u.has_jsonc_features,
    }
}

fn mutate_result_into(m: legacy::MutateResult) -> MutateResult {
    MutateResult {
        text:               m.text,
        parse_error:        m.parse_error,
        root_kind:          m.root_kind.map(kind_to_string),
        child_count:        m.child_count,
        can_undo:           m.can_undo,
        can_redo:           m.can_redo,
        has_jsonc_features: m.has_jsonc_features,
    }
}
