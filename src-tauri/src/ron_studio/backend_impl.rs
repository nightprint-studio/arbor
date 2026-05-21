//! `RonBackend` — `StudioFormatBackend` implementation for RON.
//!
//! Wraps the existing `RonStudioRegistry` behind a `Mutex` so the
//! trait can expose `&self` methods. Convert-and-forward to the
//! legacy registry methods + map RON-specific types (`NodeKind`,
//! `NodeView`, `DiffHunk`, …) into the format-agnostic shapes from
//! `studio::format::types`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use std::collections::{BTreeMap, HashSet};

use crate::error::AppError;
use crate::ron_studio::{
    self as legacy, schema, NodeKind, PrimitiveValue, RonStudioRegistry,
};
use crate::studio::edit_expr::{self, Value as ExprValue};
use crate::studio::format::{
    backend::StudioFormatBackend,
    descriptor::{
        CrossRefScope, FormatDescriptor, IconRef, KindStyle, KindTone,
        NullPolicy, QuerySyntax, SchemaSourceKind,
    },
    errors::{StudioError, StudioResult},
    types::{
        BulkEditAction, BulkEditFailure, BulkEditLiteral, BulkEditOpenDoc,
        BulkEditPreview, BulkEditResult, BulkEditScope, BulkEditSite,
        BulkEditValueSource, CrateProbe, DiffHunk, DiffLine, DiffLineKind,
        DiffStatus, DiffTreeNode, DocSnapshot, EncodingInfo, FileEntry,
        MutateResult, NodeView, ParseResult, QueryHit, RenameCollision,
        RenameDirtyBlocker, RenameFailure, RenameOpenDoc, RenamePreview,
        RenameResult, RenameSite, RenameSiteScope, Schema, SchemaHint,
        SchemaHintOrigin, StudioMutation, TypeSource, UpdateResult,
    },
};

pub struct RonBackend {
    regs:       Mutex<RonStudioRegistry>,
    descriptor: FormatDescriptor,
}

impl RonBackend {
    pub fn new() -> Self {
        Self {
            regs:       Mutex::new(RonStudioRegistry::default()),
            descriptor: build_descriptor(),
        }
    }

    fn lock(&self) -> StudioResult<std::sync::MutexGuard<'_, RonStudioRegistry>> {
        self.regs
            .lock()
            .map_err(|_| StudioError::App(AppError::Other("ron_studio registry poisoned".into())))
    }
}

impl Default for RonBackend {
    fn default() -> Self { Self::new() }
}

/// Public factory used by `lib.rs::run()` to populate the registry.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    Arc::new(RonBackend::new())
}

#[async_trait]
impl StudioFormatBackend for RonBackend {
    fn descriptor(&self) -> &FormatDescriptor { &self.descriptor }

    // ── Lifecycle ────────────────────────────────────────────────────

    async fn parse(
        &self,
        text:        String,
        source_path: Option<String>,
        encoding:    EncodingInfo,
    ) -> StudioResult<ParseResult> {
        let result = self.lock()?.parse(
            text,
            source_path,
            encoding.label.clone(),
            encoding.had_bom,
        )?;
        Ok(parse_result_into(result, encoding))
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
        Ok(self.lock()?.raw_original(doc_id)?)
    }

    fn raw_current(&self, doc_id: &str) -> StudioResult<String> {
        Ok(self.lock()?.raw_current(doc_id)?)
    }

    fn format_doc(&self, doc_id: &str) -> StudioResult<String> {
        Ok(self.lock()?.format(doc_id)?)
    }

    fn get_indent(&self, doc_id: &str) -> StudioResult<String> {
        Ok(self.lock()?.get_indent(doc_id)?)
    }

    fn set_indent(&self, doc_id: &str, indent: String) -> StudioResult<()> {
        self.lock()?.set_indent(doc_id, indent)?;
        Ok(())
    }

    // ── Tree navigation ──────────────────────────────────────────────

    fn get_root(&self, doc_id: &str) -> StudioResult<Option<NodeView>> {
        Ok(self.lock()?.get_root(doc_id)?.map(node_view_into))
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
        Ok(self.lock()?.get_value_pretty(doc_id, &path)?)
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
        let legacy_res = match mutation {
            StudioMutation::SetPrimitive { path, value } => {
                let primitive: PrimitiveValue = serde_json::from_value(value)
                    .map_err(|e| AppError::Other(format!("set_primitive value: {e}")))?;
                reg.mutate_primitive(doc_id, &path, primitive)?
            }
            StudioMutation::ToggleOption { path } => {
                reg.toggle_option(doc_id, &path)?
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
            StudioMutation::MoveItem { path, delta } => {
                reg.move_item(doc_id, &path, delta)?
            }
        };
        Ok(mutate_result_into(legacy_res))
    }

    // ── Diff ─────────────────────────────────────────────────────────

    fn diff(&self, doc_id: &str) -> StudioResult<Vec<DiffHunk>> {
        Ok(self.lock()?.diff(doc_id)?.into_iter().map(diff_hunk_into).collect())
    }

    fn tree_diff(&self, doc_id: &str) -> StudioResult<DiffTreeNode> {
        Ok(diff_tree_into(self.lock()?.tree_diff(doc_id)?))
    }

    // ── History ──────────────────────────────────────────────────────

    fn undo(&self, doc_id: &str) -> StudioResult<MutateResult> {
        Ok(mutate_result_into(self.lock()?.undo(doc_id)?))
    }

    fn redo(&self, doc_id: &str) -> StudioResult<MutateResult> {
        Ok(mutate_result_into(self.lock()?.redo(doc_id)?))
    }

    fn history_state(&self, doc_id: &str) -> StudioResult<(bool, bool)> {
        Ok(self.lock()?.history_state(doc_id)?)
    }

    // ── Snapshot & persistence ───────────────────────────────────────

    fn snapshot(&self, doc_id: &str) -> StudioResult<DocSnapshot> {
        Ok(doc_snapshot_into(self.lock()?.snapshot(doc_id)?))
    }

    fn source_path(&self, doc_id: &str) -> StudioResult<Option<String>> {
        Ok(self.lock()?.source_path(doc_id)?)
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
        let entries = tokio::task::spawn_blocking(move || legacy::list_ron_files(&folder))
            .await
            .map_err(|e| AppError::Other(format!("list_ron_files join: {e}")))??;
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
        Ok(self.lock()?.to_json(doc_id)?)
    }

    fn from_json(&self, doc_id: &str, json_text: String) -> StudioResult<String> {
        Ok(self.lock()?.from_json(doc_id, &json_text)?)
    }

    // ── Schema (RON binds to Rust source via syn) ────────────────────

    async fn schema_probe(&self, source: String) -> StudioResult<CrateProbe> {
        let probe = tokio::task::spawn_blocking(move || schema::probe(&source))
            .await
            .map_err(|e| AppError::Other(format!("schema probe join: {e}")))??;
        Ok(probe)
    }

    async fn schema_load(
        &self,
        source:         String,
        root_canonical: String,
    ) -> StudioResult<Schema> {
        let schema = tokio::task::spawn_blocking(move || schema::load(&source, &root_canonical))
            .await
            .map_err(|e| AppError::Other(format!("schema load join: {e}")))??;
        Ok(schema)
    }

    async fn schema_view_source(
        &self,
        source:         String,
        canonical_path: String,
    ) -> StudioResult<TypeSource> {
        let ts = tokio::task::spawn_blocking(move || schema::get_type_source(&source, &canonical_path))
            .await
            .map_err(|e| AppError::Other(format!("schema view-source join: {e}")))??;
        Ok(ts)
    }

    // ── F12 — Rename refactor ────────────────────────────────────────

    async fn rename_preview(
        &self,
        repo_root:      String,
        old_value:      String,
        new_value_hint: Option<String>,
        open_docs:      Vec<RenameOpenDoc>,
    ) -> StudioResult<RenamePreview> {
        if old_value.is_empty() {
            return Err(StudioError::App(AppError::Other(
                "Rename target value is empty".into(),
            )));
        }
        let preview = tokio::task::spawn_blocking(move || -> StudioResult<RenamePreview> {
            // Smart reindex — incremental refresh keeps the on-disk
            // index honest with the user's working tree without
            // re-parsing every file. Falls back to a fresh scan when
            // the index is empty (first run after install / cleared
            // state) or fails (filesystem hiccup).
            let idx = match crate::studio::index::refresh(&repo_root, None) {
                Ok(i)  => i,
                Err(e) => {
                    tracing::warn!("rename_preview: index refresh failed, falling back to fresh scan ({e})");
                    crate::studio::index::load(&repo_root)
                }
            };

            // Collect def + ref sites whose value matches `old_value`.
            // Dedupe `(absolute_path, field_path)` because the index
            // can occasionally surface the same site twice (rare, but
            // a re-parse-races defensive guard).
            let mut sites:    Vec<RenameSite> = Vec::new();
            let mut seen_key: HashSet<(String, Vec<String>)> = HashSet::new();
            for d in crate::studio::index::aggregate_cross_refs(&idx) {
                if d.id_value != old_value { continue; }
                let key = (d.absolute_path.clone(), d.def_path.clone());
                if !seen_key.insert(key) { continue; }
                sites.push(RenameSite {
                    absolute_path: d.absolute_path,
                    relative_path: d.relative_path,
                    file_name:     d.file_name,
                    field_path:    d.def_path,
                    key_name:      d.def_field,
                    scope:         RenameSiteScope::Definition,
                    preview:       String::new(),
                });
            }
            for u in crate::studio::index::aggregate_usages(&idx, &old_value) {
                let key = (u.absolute_path.clone(), u.field_path.clone());
                if !seen_key.insert(key) { continue; }
                sites.push(RenameSite {
                    absolute_path: u.absolute_path,
                    relative_path: u.relative_path,
                    file_name:     u.file_name,
                    field_path:    u.field_path,
                    key_name:      u.key_name,
                    scope:         RenameSiteScope::Reference,
                    preview:       String::new(),
                });
            }
            // Stable order: by file then by depth-first path. The FE
            // groups by file in the preview list, so deterministic
            // ordering keeps the modal state predictable across
            // re-opens of the same target.
            sites.sort_by(|a, b| a.relative_path
                .cmp(&b.relative_path)
                .then_with(|| a.field_path.cmp(&b.field_path)));

            // Best-effort line-snippet preview — read each file once and
            // pull a short value-context line. Never fatal: a missing
            // file just leaves the preview empty.
            let mut file_text_cache: BTreeMap<String, String> = BTreeMap::new();
            for site in sites.iter_mut() {
                let text = file_text_cache
                    .entry(site.absolute_path.clone())
                    .or_insert_with(|| read_file_to_string(&site.absolute_path));
                site.preview = synth_preview_line(text, &site.key_name, &old_value);
            }

            // Collisions: when the user already gave a `new_value_hint`,
            // surface every existing def whose value equals that hint
            // so the modal can warn about the post-rename namespace
            // collision (FROZEN F12 — sticky warning, NOT a hard block).
            let collisions = match new_value_hint.as_deref() {
                Some(hint) if !hint.is_empty() && hint != old_value => {
                    let mut out: Vec<RenameCollision> = Vec::new();
                    for d in crate::studio::index::aggregate_cross_refs(&idx) {
                        if d.id_value != hint { continue; }
                        out.push(RenameCollision {
                            absolute_path: d.absolute_path,
                            relative_path: d.relative_path,
                            field_path:    d.def_path,
                            key_name:      d.def_field,
                        });
                    }
                    out
                }
                _ => Vec::new(),
            };

            // Dirty blockers: any open doc whose source path matches an
            // affected file AND has unsaved changes per the FE state.
            // FROZEN F12: refactor blocks until the user saves or
            // discards. Match on canonicalised path strings so case-only
            // / separator-only differences don't slip through.
            let affected_paths: HashSet<String> = sites
                .iter()
                .map(|s| canonicalise_path_key(&s.absolute_path))
                .collect();
            let mut dirty_blockers: Vec<RenameDirtyBlocker> = open_docs
                .into_iter()
                .filter(|d| d.dirty)
                .filter(|d| match &d.source_path {
                    Some(p) => affected_paths.contains(&canonicalise_path_key(p)),
                    None    => false,
                })
                .map(|d| RenameDirtyBlocker {
                    doc_id:      d.doc_id,
                    source_path: d.source_path,
                })
                .collect();
            dirty_blockers.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));

            Ok(RenamePreview {
                sites,
                dirty_blockers,
                collisions,
            })
        })
        .await
        .map_err(|e| AppError::Other(format!("rename_preview join: {e}")))??;
        Ok(preview)
    }

    // ── F13 — Query-driven bulk edit ─────────────────────────────────

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
        // Compile the expression once up-front so the per-site eval
        // loop doesn't pay the cost. Compile errors land in the
        // top-level `expression_error` banner; per-site eval errors
        // surface inside the site list.
        let compiled = match (&action, &value_source) {
            (BulkEditAction::Set, Some(BulkEditValueSource::Expression { source })) => {
                match edit_expr::compile(source) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        return Ok(BulkEditPreview {
                            sites:            Vec::new(),
                            dirty_blockers:   Vec::new(),
                            expression_error: Some(e.0),
                        });
                    }
                }
            }
            _ => None,
        };

        match scope {
            BulkEditScope::ActiveDoc => {
                let (current_text, source_path) = {
                    let reg = self.lock()?;
                    (reg.raw_current(&doc_id)?, reg.source_path(&doc_id)?)
                };
                let mut sites: Vec<BulkEditSite> = Vec::new();
                if let Ok(root) = legacy::ast::parse(&current_text) {
                    let hits = legacy::query_ast(&root, &query)?;
                    for h in hits {
                        let v = match legacy::resolve_path(&root, &h.path) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let (abs_path, rel_path, file_name) = synth_active_doc_paths(&source_path);
                        sites.push(build_site_for_preview(
                            &abs_path, &rel_path, &file_name, &h.path, v,
                            &action, &value_source, compiled.as_ref(),
                        ));
                    }
                }
                Ok(BulkEditPreview {
                    sites,
                    dirty_blockers:   Vec::new(),
                    expression_error: None,
                })
            }
            BulkEditScope::ProjectWide => {
                let query    = query.clone();
                let action   = action;
                let value    = value_source.clone();
                let compiled = compiled.clone();
                tokio::task::spawn_blocking(move || -> StudioResult<BulkEditPreview> {
                    let mut sites: Vec<BulkEditSite> = Vec::new();
                    let files = crate::studio::scan_repo(
                        &repo_root,
                        &[crate::studio::StudioFileKind::Ron],
                    )?;
                    for f in &files {
                        if f.excluded { continue; }
                        let Ok(bytes) = std::fs::read(&f.absolute_path) else { continue; };
                        let (text, _enc, _had_bom) =
                            crate::git::encoding::decode_bytes_full(&bytes);
                        let Ok(root) = legacy::ast::parse(&text) else { continue; };
                        let hits = legacy::query_ast(&root, &query)?;
                        for h in hits {
                            let Ok(v) = legacy::resolve_path(&root, &h.path) else { continue; };
                            sites.push(build_site_for_preview(
                                &f.absolute_path,
                                &f.relative_path,
                                &f.name,
                                &h.path, v,
                                &action, &value, compiled.as_ref(),
                            ));
                        }
                    }
                    // Stable order: by file then by depth-first path.
                    sites.sort_by(|a, b|
                        a.relative_path.cmp(&b.relative_path)
                            .then_with(|| a.field_path.cmp(&b.field_path))
                    );

                    // Dirty blockers — any open doc whose source matches
                    // an affected file AND is dirty. Same logic as F12.
                    let affected_paths: HashSet<String> = sites
                        .iter()
                        .map(|s| canonicalise_path_key(&s.absolute_path))
                        .collect();
                    let mut dirty_blockers: Vec<RenameDirtyBlocker> = open_docs
                        .into_iter()
                        .filter(|d| d.dirty)
                        .filter(|d| match &d.source_path {
                            Some(p) => affected_paths.contains(&canonicalise_path_key(p)),
                            None    => false,
                        })
                        .map(|d| RenameDirtyBlocker {
                            doc_id:      d.doc_id,
                            source_path: d.source_path,
                        })
                        .collect();
                    dirty_blockers.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));

                    Ok(BulkEditPreview {
                        sites,
                        dirty_blockers,
                        expression_error: None,
                    })
                })
                .await
                .map_err(|e| AppError::Other(format!("bulk_edit_preview join: {e}")))?
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
        // Compile expression up-front (same gate as preview).
        let compiled = match (&action, &value_source) {
            (BulkEditAction::Set, Some(BulkEditValueSource::Expression { source })) => {
                Some(edit_expr::compile(source).map_err(|e|
                    AppError::Other(format!("Expression compile error: {e}"))
                )?)
            }
            _ => None,
        };

        match scope {
            BulkEditScope::ActiveDoc => {
                // Active-doc flow uses the registry's history-aware
                // mutate pipeline so the bulk edit is a single undo
                // entry. Builds ops by re-reading each site's value
                // from the doc's current AST (defensive — the FE may
                // have stale previews if the doc was edited between
                // preview and apply).
                let (ops, applied, skipped) = {
                    let reg = self.lock()?;
                    let current = reg.raw_current(&doc_id)?;
                    let root = legacy::ast::parse(&current)
                        .map_err(|e| AppError::Other(format!("parse: {e}")))?;
                    build_ops_from_sites(
                        &root, &sites, &action, &value_source, compiled.as_ref(),
                    )?
                };
                let state = if ops.is_empty() {
                    None
                } else {
                    let mut reg = self.lock()?;
                    Some(mutate_result_into(reg.apply_bulk_edits_doc(&doc_id, &ops)?))
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
                // Defensive dirty re-check (same as F12 rename_apply).
                let affected_paths: HashSet<String> = sites
                    .iter()
                    .map(|s| canonicalise_path_key(&s.absolute_path))
                    .collect();
                let any_dirty = open_docs.iter().any(|d| {
                    d.dirty
                        && d.source_path
                            .as_ref()
                            .map(|p| affected_paths.contains(&canonicalise_path_key(p)))
                            .unwrap_or(false)
                });
                if any_dirty {
                    return Err(StudioError::App(AppError::Other(
                        "Some affected files have unsaved changes. Save or discard first.".into(),
                    )));
                }
                let _ = repo_root;

                let action    = action;
                let value_src = value_source.clone();
                let compiled  = compiled.clone();

                let result = tokio::task::spawn_blocking(move || -> StudioResult<BulkEditResult> {
                    // Group sites by file so each file is parsed once.
                    let mut by_file: BTreeMap<String, Vec<BulkEditSite>> = BTreeMap::new();
                    for s in sites {
                        by_file.entry(s.absolute_path.clone()).or_default().push(s);
                    }

                    // Phase A — parse + apply edits per-file in memory.
                    // FROZEN F12 atomic rule applies: any failure here
                    // aborts the batch BEFORE any disk write.
                    struct PendingWrite {
                        abs_path:       String,
                        new_text:       String,
                        encoding_label: String,
                        had_bom:        bool,
                    }
                    let mut pending:    Vec<PendingWrite> = Vec::with_capacity(by_file.len());
                    let mut applied_n: usize             = 0;
                    let mut skipped_n: usize             = 0;
                    for (abs_path, sites_for_file) in by_file {
                        let bytes = std::fs::read(&abs_path).map_err(|e| {
                            AppError::Other(format!("Read {abs_path}: {e}"))
                        })?;
                        let (text, enc, had_bom) =
                            crate::git::encoding::decode_bytes_full(&bytes);
                        let root = legacy::ast::parse(&text)
                            .map_err(|e| AppError::Other(format!("parse {abs_path}: {e}")))?;
                        let (ops, a, s) = build_ops_from_sites(
                            &root, &sites_for_file, &action, &value_src, compiled.as_ref(),
                        )?;
                        applied_n += a;
                        skipped_n += s;
                        if ops.is_empty() { continue; }
                        let mut root_mut = root;
                        legacy::apply_bulk_edits_inplace(&mut root_mut, &ops)
                            .map_err(|e| AppError::Other(format!(
                                "Apply edits to {abs_path}: {e}",
                            )))?;
                        let new_text = legacy::ast::to_pretty_string_with(&root_mut, "  ");
                        pending.push(PendingWrite {
                            abs_path,
                            new_text,
                            encoding_label: enc.name().to_string(),
                            had_bom,
                        });
                    }

                    // Phase B — flush sequentially. Mid-batch failures
                    // surface as `failed_files`; no auto rollback of
                    // already-written files (FROZEN F12).
                    let mut written: Vec<String>          = Vec::new();
                    let mut failed:  Vec<BulkEditFailure> = Vec::new();
                    for w in pending {
                        match legacy::write_to_disk(
                            &w.abs_path, &w.new_text, &w.encoding_label, w.had_bom,
                        ) {
                            Ok(())  => written.push(w.abs_path),
                            Err(e)  => failed.push(BulkEditFailure {
                                absolute_path: w.abs_path,
                                message:       e.to_string(),
                            }),
                        }
                    }
                    Ok(BulkEditResult {
                        written_files:    written,
                        failed_files:     failed,
                        applied_sites:    applied_n,
                        skipped_sites:    skipped_n,
                        active_doc_state: None,
                    })
                })
                .await
                .map_err(|e| AppError::Other(format!("bulk_edit_apply join: {e}")))??;
                Ok(result)
            }
        }
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
            return Err(StudioError::App(AppError::Other(
                "New value is empty".into(),
            )));
        }
        if new_value == old_value {
            return Err(StudioError::App(AppError::Other(
                "New value equals old value — nothing to rename".into(),
            )));
        }
        if sites.is_empty() {
            return Err(StudioError::App(AppError::Other(
                "No sites selected for rename".into(),
            )));
        }

        // Defensive dirty re-check — the BE doesn't trust the FE-side
        // gate alone; if a doc went dirty between preview and apply we
        // surface it as an error before any disk write.
        let affected_paths: HashSet<String> = sites
            .iter()
            .map(|s| canonicalise_path_key(&s.absolute_path))
            .collect();
        let any_dirty = open_docs.iter().any(|d| {
            d.dirty
                && d.source_path
                    .as_ref()
                    .map(|p| affected_paths.contains(&canonicalise_path_key(p)))
                    .unwrap_or(false)
        });
        if any_dirty {
            return Err(StudioError::App(AppError::Other(
                "Some affected files have unsaved changes. Save or discard first.".into(),
            )));
        }

        let _ = repo_root; // repo_root not needed directly — sites already carry absolute paths

        let result = tokio::task::spawn_blocking(move || -> StudioResult<RenameResult> {
            // Group sites by absolute_path so each file is parsed once
            // and the sites are applied in a single AST mutation pass.
            let mut by_file: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
            for s in &sites {
                by_file.entry(s.absolute_path.clone()).or_default().push(s.field_path.clone());
            }

            // Phase A — parse + rename all files in memory. Any failure
            // here aborts the batch BEFORE any disk write (FROZEN F12).
            // Each file remembers its own encoding (FROZEN F16) so the
            // flush phase below re-encodes per-file, never globally.
            struct PendingWrite {
                abs_path:       String,
                new_text:       String,
                encoding_label: String,
                had_bom:        bool,
            }
            let mut pending: Vec<PendingWrite> = Vec::with_capacity(by_file.len());
            for (abs_path, paths) in by_file {
                let bytes = std::fs::read(&abs_path).map_err(|e| {
                    AppError::Other(format!("Read {abs_path}: {e}"))
                })?;
                let (text, enc, had_bom) =
                    crate::git::encoding::decode_bytes_full(&bytes);
                let new_text = crate::ron_studio::apply_string_rename(
                    &text, &paths, &new_value, "  ",
                )
                .map_err(|e| AppError::Other(format!(
                    "Rename in-memory pass failed for {abs_path}: {e}"
                )))?;
                pending.push(PendingWrite {
                    abs_path,
                    new_text,
                    encoding_label: enc.name().to_string(),
                    had_bom,
                });
            }

            // Phase B — flush sequentially. A flush failure mid-batch
            // is recorded as a per-file failure but does NOT roll back
            // already-written files (FROZEN F12).
            let mut written: Vec<String>        = Vec::new();
            let mut failed:  Vec<RenameFailure> = Vec::new();
            for w in pending {
                match crate::ron_studio::write_to_disk(
                    &w.abs_path, &w.new_text, &w.encoding_label, w.had_bom,
                ) {
                    Ok(())  => written.push(w.abs_path),
                    Err(e)  => failed.push(RenameFailure {
                        absolute_path: w.abs_path,
                        message:       e.to_string(),
                    }),
                }
            }

            Ok(RenameResult { written_files: written, failed_files: failed })
        })
        .await
        .map_err(|e| AppError::Other(format!("rename_apply join: {e}")))??;
        Ok(result)
    }
}

// ─── F12 helpers ─────────────────────────────────────────────────────

/// Best-effort UTF-8 decode for the line-preview synth. We don't need
/// F16-grade lossless decoding here — the preview line is a UI hint,
/// and any encoded byte that doesn't survive is replaced with U+FFFD.
fn read_file_to_string(abs_path: &str) -> String {
    let Ok(bytes) = std::fs::read(abs_path) else { return String::new(); };
    let (text, _, _) = crate::git::encoding::decode_bytes_full(&bytes);
    text
}

/// Synthesise a short preview snippet for the rename modal: find the
/// first line containing the key+value pair, fall back to the first
/// non-empty line, and trim to ~80 chars. Cheap heuristic — the FE
/// only needs enough context for the user to recognise the site.
fn synth_preview_line(text: &str, key: &str, value: &str) -> String {
    if text.is_empty() { return String::new(); }
    let needle1 = format!("{key}:");
    let needle2 = format!("\"{value}\"");
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() { continue; }
        if l.contains(&needle1) && l.contains(&needle2) {
            best = Some(l);
            break;
        }
        // Container-shape ref fields ("Action: [\"x\", \"y\"]") may
        // not match `key:` directly when the key sits on the previous
        // line — fall back to value-only matches.
        if best.is_none() && l.contains(&needle2) {
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

/// Canonicalise a path for cross-reference equality checks: lowercase
/// and forward-slashes. Windows treats paths case-insensitively, and
/// the FE may surface either separator depending on how the doc was
/// opened (sidebar vs disk picker).
fn canonicalise_path_key(p: &str) -> String {
    p.replace('\\', "/").to_ascii_lowercase()
}

// ─── F13 helpers ─────────────────────────────────────────────────────

/// Synthesise the (absolute, relative, file_name) tuple for an active-
/// doc site when the doc has no backing source path (paste-only doc).
/// In that case we surface a stub `"(active doc)"` so the FE can
/// still render the site list without crashing on empty paths.
fn synth_active_doc_paths(source_path: &Option<String>) -> (String, String, String) {
    match source_path {
        Some(p) => {
            let file_name = std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            (p.clone(), file_name.clone(), file_name)
        }
        None => (
            "(active doc)".to_string(),
            "(active doc)".to_string(),
            "(active doc)".to_string(),
        ),
    }
}

/// Build one preview row from a `(path, ast_value)` pair + the chosen
/// action / value source. Surfaces the site even when it will be
/// skipped so the user sees what got dropped and why.
fn build_site_for_preview(
    abs_path:     &str,
    rel_path:     &str,
    file_name:    &str,
    field_path:   &[String],
    target:       &legacy::ast::RonAst,
    action:       &BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&edit_expr::CompiledExpr>,
) -> BulkEditSite {
    use legacy::ast::RonAst;
    let kind   = kind_to_string(legacy::node_kind(target));
    let preview = legacy::preview_for(target);
    let mut will_skip   = false;
    let mut skip_reason = String::new();
    let mut new_preview = String::new();

    // Container nodes are universally rejected for `set` (FROZEN F13).
    // `delete` is allowed on container siblings (the parent removes
    // the field/item that holds the container).
    let is_container = matches!(target,
        RonAst::Struct { .. } | RonAst::Tuple { .. } | RonAst::List(_) | RonAst::Map(_)
    );

    match action {
        BulkEditAction::Delete => {
            if field_path.is_empty() {
                will_skip = true;
                skip_reason = "Cannot delete the document root".into();
            } else if matches!(target, RonAst::Option(_)) {
                // For Option targets, delete becomes "set to None" per
                // FROZEN F13. The pretty-printed preview reflects that.
                new_preview = "None".into();
            } else {
                new_preview = "(removed)".into();
            }
        }
        BulkEditAction::Set => {
            if is_container {
                will_skip = true;
                skip_reason = "`set` cannot target a container node — descend deeper into the query".into();
            } else {
                // Compute the new value using either the literal or the
                // expression. Eval errors and type mismatches show as
                // per-site skips with visible reason.
                match compute_new_value(target, value_source, compiled) {
                    Ok(set_val) => new_preview = render_set_preview(target, &set_val),
                    Err(reason) => {
                        will_skip = true;
                        skip_reason = reason;
                    }
                }
            }
        }
    }

    BulkEditSite {
        absolute_path: abs_path.to_string(),
        relative_path: rel_path.to_string(),
        file_name:     file_name.to_string(),
        field_path:    field_path.to_vec(),
        kind,
        old_preview:   preview,
        new_preview,
        will_skip,
        skip_reason,
    }
}

/// Compute the `BulkSetValue` for one site given the chosen value
/// source. Returns `Err(reason)` when the site can't be set (eval
/// error, type mismatch, RON-null on non-option, …).
fn compute_new_value(
    target:       &legacy::ast::RonAst,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&edit_expr::CompiledExpr>,
) -> Result<legacy::BulkSetValue, String> {
    use legacy::ast::RonAst;
    let raw_value: ExprValue = match value_source {
        Some(BulkEditValueSource::Literal { literal }) => match literal {
            BulkEditLiteral::String(s) => ExprValue::String(s.clone()),
            BulkEditLiteral::Number(n) => ExprValue::Number(*n),
            BulkEditLiteral::Bool(b)   => ExprValue::Bool(*b),
            BulkEditLiteral::Null      => ExprValue::Null,
        }
        Some(BulkEditValueSource::Expression { .. }) => {
            let compiled = compiled.ok_or_else(|| "internal: compiled expression missing".to_string())?;
            let old = ast_to_eval_value(target)
                .ok_or_else(|| "container node — cannot bind `old`".to_string())?;
            match compiled.eval(&old) {
                Ok(v) => v,
                Err(e) => return Err(e.0),
            }
        }
        None => return Err("Value source missing for `set` action".into()),
    };

    // Coerce ExprValue → BulkSetValue per RON's rules.
    // - On Option targets: any concrete value wraps in Some; Null → None
    // - On non-option: Null is rejected ("RON has no null")
    // - String/Bool require matching receiver kind (except UnitVariant
    //   which can accept a string for rename-style ops)
    // - Number → Int when receiver is Int and value is integral; Float
    //   otherwise. Float on Int receiver promotes to Float when
    //   non-integral.
    use legacy::BulkSetValue as B;
    let is_option = matches!(target, RonAst::Option(_));
    if is_option {
        return Ok(match raw_value {
            ExprValue::Null      => B::Null,
            ExprValue::Bool(b)   => B::Bool(b),
            ExprValue::Number(n) => coerce_number_for_target(target, n),
            ExprValue::String(s) => B::String(s),
        });
    }
    match (&raw_value, target) {
        (ExprValue::Null, _) => Err(
            "RON has no null — use `delete` or guard with `?? \"\"` for a fallback".into()
        ),
        (ExprValue::Bool(b),   RonAst::Bool(_))   => Ok(B::Bool(*b)),
        (ExprValue::String(s), RonAst::String(_)) => Ok(B::String(s.clone())),
        (ExprValue::String(s), RonAst::Char(_))   => {
            let mut it = s.chars();
            let Some(c) = it.next() else { return Err("Empty string cannot fill a char field".into()); };
            if it.next().is_some() { return Err("String with >1 char cannot fill a char field".into()); }
            // Encode as a one-char string and let RON accept it — the
            // AST stores chars distinctly so we need BulkSetValue::String
            // → but RON Char is a separate variant. Refuse for now.
            let _ = c;
            Err("Char target — bulk-edit set on char is not implemented in v1".into())
        }
        (ExprValue::Number(n), RonAst::Int(_) | RonAst::Float(_)) => {
            Ok(coerce_number_for_target(target, *n))
        }
        (ExprValue::String(s), RonAst::UnitVariant(_)) => Ok(B::String(s.clone())),
        (v, t) => Err(format!(
            "Type mismatch: {} value cannot fill a {} field",
            v.type_name(),
            kind_to_string(legacy::node_kind(t)),
        )),
    }
}

/// Number coercion: Int receiver + integral value → Int; otherwise
/// Float. Lets `old + 1` on an integer field stay integer, and
/// `old * 1.5` promote it to float (matches user expectation).
fn coerce_number_for_target(target: &legacy::ast::RonAst, n: f64) -> legacy::BulkSetValue {
    use legacy::ast::RonAst;
    use legacy::BulkSetValue as B;
    let receiver_is_int = matches!(target, RonAst::Int(_));
    if receiver_is_int && n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        B::Int(n as i64)
    } else if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        // Float receiver but integral value → keep float for stable
        // round-trip on the source kind.
        B::Float(n)
    } else {
        B::Float(n)
    }
}

/// Render a tight preview of the would-be new value for the UI. Same
/// shape as `preview_for(&ast)` so the modal's old/new line looks
/// consistent.
fn render_set_preview(target: &legacy::ast::RonAst, v: &legacy::BulkSetValue) -> String {
    use legacy::BulkSetValue as B;
    let is_option = matches!(target, legacy::ast::RonAst::Option(_));
    let inner = match v {
        B::String(s) => format!("\"{s}\""),
        B::Int(i)    => i.to_string(),
        B::Float(f)  => format!("{f}"),
        B::Bool(b)   => b.to_string(),
        B::Null      => "None".into(),
    };
    if is_option && !matches!(v, B::Null) {
        format!("Some({inner})")
    } else {
        inner
    }
}

/// Map a RON leaf into an `edit_expr::Value` for expression eval. None
/// for container kinds (which the preview rejects up front, so this
/// never returns None at apply time for accepted sites).
fn ast_to_eval_value(v: &legacy::ast::RonAst) -> Option<ExprValue> {
    use legacy::ast::RonAst;
    match v {
        RonAst::String(s)            => Some(ExprValue::String(s.clone())),
        RonAst::Int(i)               => Some(ExprValue::Number(*i as f64)),
        RonAst::Float(f)             => Some(ExprValue::Number(*f)),
        RonAst::Bool(b)              => Some(ExprValue::Bool(*b)),
        RonAst::Char(c)              => Some(ExprValue::String(c.to_string())),
        RonAst::UnitVariant(n)       => Some(ExprValue::String(n.clone())),
        RonAst::Option(None)         => Some(ExprValue::Null),
        RonAst::Option(Some(inner))  => ast_to_eval_value(inner.as_ref()),
        RonAst::Unit                 => Some(ExprValue::Null),
        _ => None,
    }
}

/// Resolve each FE-submitted site against the live AST and build the
/// list of `(path, BulkEditOp)`s to feed `apply_bulk_edits_inplace`.
/// Skipped sites (eval errors, container hits on `set`, type
/// mismatches, …) are counted but not enqueued. Returns
/// `(ops, applied_count, skipped_count)`.
fn build_ops_from_sites(
    root:         &legacy::ast::RonAst,
    sites:        &[BulkEditSite],
    action:       &BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&edit_expr::CompiledExpr>,
) -> StudioResult<(Vec<(Vec<String>, legacy::BulkEditOp)>, usize, usize)> {
    let mut ops:     Vec<(Vec<String>, legacy::BulkEditOp)> = Vec::with_capacity(sites.len());
    let mut applied: usize = 0;
    let mut skipped: usize = 0;
    for site in sites {
        if site.will_skip {
            skipped += 1;
            continue;
        }
        let target = match legacy::resolve_path(root, &site.field_path) {
            Ok(v) => v,
            Err(_) => { skipped += 1; continue; }
        };
        match action {
            BulkEditAction::Delete => {
                ops.push((site.field_path.clone(), legacy::BulkEditOp::Delete));
                applied += 1;
            }
            BulkEditAction::Set => {
                match compute_new_value(target, value_source, compiled) {
                    Ok(v) => {
                        ops.push((site.field_path.clone(), legacy::BulkEditOp::Set(v)));
                        applied += 1;
                    }
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    Ok((ops, applied, skipped))
}

// ─── Type conversions ────────────────────────────────────────────────

fn kind_to_str(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Struct      => "struct",
        NodeKind::NamedStruct => "named_struct",
        NodeKind::Tuple       => "tuple",
        NodeKind::NamedTuple  => "named_tuple",
        NodeKind::UnitVariant => "unit_variant",
        NodeKind::Map         => "map",
        NodeKind::List        => "list",
        NodeKind::String      => "string",
        NodeKind::Char        => "char",
        NodeKind::Number      => "number",
        NodeKind::Bool        => "bool",
        NodeKind::Option      => "option",
        NodeKind::Unit        => "unit",
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
        variant_tag: v.variant_tag,
    }
}

fn query_hit_into(h: legacy::RonQueryHit) -> QueryHit {
    QueryHit {
        path:        h.path,
        kind:        kind_to_string(h.kind),
        preview:     h.preview,
        variant_tag: h.variant_tag,
    }
}

fn parse_result_into(p: legacy::ParseResult, encoding: EncodingInfo) -> ParseResult {
    ParseResult {
        doc_id:      p.doc_id,
        size_bytes:  p.size_bytes,
        source_path: p.source_path,
        original:    p.original,
        parse_error: p.parse_error,
        root_kind:   p.root_kind.map(kind_to_string),
        child_count: p.child_count,
        schema_hint: p.schema_hint.map(schema_hint_into),
        encoding,
        // Phase 3.d additions — JSON-specific, RON never sets them.
        stream_mode:        false,
        is_jsonc:           false,
        has_jsonc_features: false,
    }
}

fn update_result_into(u: legacy::UpdateResult) -> UpdateResult {
    UpdateResult {
        parse_error:        u.parse_error,
        root_kind:          u.root_kind.map(kind_to_string),
        child_count:        u.child_count,
        can_undo:           u.can_undo,
        can_redo:           u.can_redo,
        has_jsonc_features: false,
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
        has_jsonc_features: false,
    }
}

fn schema_hint_into(h: legacy::SchemaHint) -> SchemaHint {
    SchemaHint {
        rs_file:   h.rs_file,
        root_type: h.root_type,
        origin:    match h.origin {
            legacy::SchemaHintOrigin::Directive => SchemaHintOrigin::Directive,
            legacy::SchemaHintOrigin::Sidecar   => SchemaHintOrigin::Sidecar,
        },
    }
}

fn diff_hunk_into(h: legacy::DiffHunk) -> DiffHunk {
    DiffHunk {
        old_start: h.old_start,
        old_count: h.old_count,
        new_start: h.new_start,
        new_count: h.new_count,
        lines:     h.lines.into_iter().map(diff_line_into).collect(),
    }
}

fn diff_line_into(l: legacy::DiffLine) -> DiffLine {
    DiffLine {
        kind:     match l.kind {
            legacy::DiffLineKind::Context => DiffLineKind::Context,
            legacy::DiffLineKind::Add     => DiffLineKind::Add,
            legacy::DiffLineKind::Del     => DiffLineKind::Del,
        },
        old_line: l.old_line,
        new_line: l.new_line,
        text:     l.text,
    }
}

fn diff_status_into(s: legacy::DiffStatus) -> DiffStatus {
    match s {
        legacy::DiffStatus::Unchanged => DiffStatus::Unchanged,
        legacy::DiffStatus::Added     => DiffStatus::Added,
        legacy::DiffStatus::Removed   => DiffStatus::Removed,
        legacy::DiffStatus::Modified  => DiffStatus::Modified,
        legacy::DiffStatus::Partial   => DiffStatus::Partial,
    }
}

fn diff_tree_into(n: legacy::DiffTreeNode) -> DiffTreeNode {
    DiffTreeNode {
        key:            n.key,
        path:           n.path,
        status:         diff_status_into(n.status),
        kind_before:    n.kind_before.map(kind_to_string),
        kind_after:     n.kind_after.map(kind_to_string),
        preview_before: n.preview_before,
        preview_after:  n.preview_after,
        tag_before:     n.tag_before,
        tag_after:      n.tag_after,
        children:       n.children.into_iter().map(diff_tree_into).collect(),
        change_count:   n.change_count,
    }
}

fn doc_snapshot_into(s: legacy::DocSnapshot) -> DocSnapshot {
    DocSnapshot {
        doc_id:      s.doc_id,
        source_path: s.source_path,
        size_bytes:  s.size_bytes,
        original:    s.original,
        current:     s.current,
        parse_error: s.parse_error,
        root_kind:   s.root_kind.map(kind_to_string),
        child_count: s.child_count,
        can_undo:    s.can_undo,
        can_redo:    s.can_redo,
        indent:      s.indent,
    }
}

// ─── Descriptor ──────────────────────────────────────────────────────

fn build_descriptor() -> FormatDescriptor {
    use std::collections::BTreeMap;

    let mut kind_palette = BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    kind_palette.insert("struct".into(),       entry("struct",        KindTone::Info));
    kind_palette.insert("named_struct".into(), entry("named struct",  KindTone::Accent));
    kind_palette.insert("tuple".into(),        entry("tuple",         KindTone::Info));
    kind_palette.insert("named_tuple".into(),  entry("named tuple",   KindTone::Accent));
    kind_palette.insert("unit_variant".into(), entry("unit variant",  KindTone::Accent));
    kind_palette.insert("map".into(),          entry("map",           KindTone::Info));
    kind_palette.insert("list".into(),         entry("list",          KindTone::Info));
    kind_palette.insert("string".into(),       entry("string",        KindTone::Success));
    kind_palette.insert("char".into(),         entry("char",          KindTone::Success));
    kind_palette.insert("number".into(),       entry("number",        KindTone::Warning));
    kind_palette.insert("bool".into(),         entry("bool",          KindTone::Warning));
    kind_palette.insert("option".into(),       entry("option",        KindTone::Muted));
    kind_palette.insert("unit".into(),         entry("unit",          KindTone::Muted));

    FormatDescriptor {
        id:                          "ron".into(),
        label:                       "RON".into(),
        file_extensions:             vec![".ron".into()],
        // No Iconify glyph for RON — the FE uses a custom inline SVG
        // shipped with `@iconify-icons/vscode-icons/file-type-ron`
        // (the RonStudioModal already imports it directly today, so
        // we don't need to embed the SVG bytes here).
        icon:                        IconRef::Iconify { name: "vscode-icons:file-type-ron".into() },

        supports_lossless_edit:      false,
        supports_comments:           false,
        supports_anchors:            false,
        null_handling:               NullPolicy::NotSupported,

        supports_streaming_mode:     false,
        streaming_threshold_kb:      None,
        streaming_setting_key:       None,

        query_syntax:                QuerySyntax::JsonPath,

        cross_ref_default_fields:    vec!["id".into(), "name".into()],
        cross_ref_scopes:            vec![CrossRefScope::Value],

        schema_sources:              vec![SchemaSourceKind::RustStruct],

        kind_palette,

        save_warnings:               Vec::new(),
        save_behavior_setting_key:   None,

        convert_to_json_supported:   true,

        supports_external_files:     true,

        supports_rename_reference:   true,

        supports_bulk_edit:          true,
    }
}
