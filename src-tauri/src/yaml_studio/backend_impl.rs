//! `YamlBackend` — `StudioFormatBackend` implementation for YAML.
//!
//! Phase 5.a scaffold + 5.b lossless edit pipeline + 5.c cross-refs,
//! F12 rename, F13 bulk edit and JSON Schema sidecar panel.
//!
//! Mutations go through `yaml_edit::Document` (rowan-based — preserves
//! comments, quote style, anchors and blank lines). Save round-trips
//! the per-doc encoding via `git::encoding::encode_for_disk_with_bom`
//! (FROZEN F16).
//!
//! Cross-refs reuse the TOML walker over `yaml_studio::parse_to_value`
//! (both formats project to `serde_json::Value`); F12 rename uses the
//! lossless `apply_string_rename` route built on `Document::set_path`;
//! F13 bulk edit composes per-site ops into a single batch and routes
//! through `mutate_with` so the doc-level apply records a single undo
//! entry. The schema panel delegates to `crate::json_studio::schema`
//! since YAML's only declared schema source is JSON Schema (no Rust
//! struct probe).

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::error::AppError;
use crate::yaml_studio::{
    self as legacy, NodeKind, YamlBulkOp, YamlSetValue, YamlStudioRegistry,
};
use crate::studio::edit_expr::{self, CompiledExpr, Value as ExprValue};
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
        BulkEditValueSource, CrateProbe, DiffHunk, DiffTreeNode, DocSnapshot,
        EncodingInfo, FileEntry, MutateResult, NodeView, ParseResult,
        QueryHit, RenameCollision, RenameDirtyBlocker, RenameFailure,
        RenameOpenDoc, RenamePreview, RenameResult, RenameSite,
        RenameSiteScope, Schema, StudioMutation, TypeSource, UpdateResult,
    },
};

pub struct YamlBackend {
    regs:       Mutex<YamlStudioRegistry>,
    descriptor: FormatDescriptor,
}

impl YamlBackend {
    pub fn new() -> Self {
        Self {
            regs:       Mutex::new(YamlStudioRegistry::default()),
            descriptor: build_descriptor(),
        }
    }

    fn lock(&self) -> StudioResult<std::sync::MutexGuard<'_, YamlStudioRegistry>> {
        self.regs
            .lock()
            .map_err(|_| StudioError::App(AppError::Other("yaml_studio registry poisoned".into())))
    }
}

impl Default for YamlBackend {
    fn default() -> Self { Self::new() }
}

/// Public factory used by `lib.rs::run()` to populate the registry.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    Arc::new(YamlBackend::new())
}

#[async_trait]
impl StudioFormatBackend for YamlBackend {
    fn descriptor(&self) -> &FormatDescriptor { &self.descriptor }

    // ── Lifecycle ────────────────────────────────────────────────────

    async fn parse(
        &self,
        text:        String,
        source_path: Option<String>,
        encoding:    EncodingInfo,
    ) -> StudioResult<ParseResult> {
        let res = self.lock()?.parse(
            text,
            source_path,
            encoding.label.clone(),
            encoding.had_bom,
        );
        Ok(ParseResult {
            doc_id:             res.doc_id,
            size_bytes:         res.size_bytes,
            source_path:        res.source_path,
            original:           String::new(),
            parse_error:        res.parse_error,
            root_kind:          res.root_kind.map(kind_to_string),
            child_count:        res.child_count,
            schema_hint:        None,
            encoding,
            stream_mode:        false,
            is_jsonc:           false,
            has_jsonc_features: false,
        })
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
        Ok(self.lock()?.pretty(doc_id)?)
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
        match self.lock()?.get_root(doc_id) {
            Ok(v)  => Ok(Some(node_view_into(v))),
            Err(_) => Ok(None),
        }
    }

    fn get_children(&self, doc_id: &str, path: Vec<String>) -> StudioResult<Vec<NodeView>> {
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
            // YAML's `null` is a first-class value, not an Option — the
            // FE never offers toggle-option for this format. Belt-and-
            // braces.
            StudioMutation::ToggleOption { .. } => {
                return Err(StudioError::unsupported("yaml", "toggle_option"));
            }
        };
        Ok(mutate_result_into(res))
    }

    // ── Diff ─────────────────────────────────────────────────────────

    fn diff(&self, doc_id: &str) -> StudioResult<Vec<DiffHunk>> {
        Ok(self.lock()?.diff(doc_id)?)
    }

    fn tree_diff(&self, doc_id: &str) -> StudioResult<DiffTreeNode> {
        Ok(self.lock()?.tree_diff(doc_id)?)
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
        Ok(self.lock()?.source_path(doc_id)?)
    }

    async fn save(
        &self,
        doc_id:      &str,
        path:        String,
        contents:    String,
        bind_to_doc: bool,
    ) -> StudioResult<()> {
        // FROZEN F16 — round-trip the per-doc encoding through save.
        let (encoding_label, had_bom) = self.lock()?.encoding_info(doc_id)?;
        legacy::write_to_disk(&path, &contents, &encoding_label, had_bom)?;
        let mut reg = self.lock()?;
        if bind_to_doc {
            reg.rebind_source(doc_id, path)?;
        }
        reg.mark_saved(doc_id)?;
        Ok(())
    }

    // ── F12 — Rename refactor (lossless via yaml_edit::set_path) ─────

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
            use crate::studio::StudioFileKind;
            let idx = match crate::studio::index::refresh_for(
                &repo_root,
                &[StudioFileKind::Yaml],
                None,
            ) {
                Ok(i)  => i,
                Err(e) => {
                    tracing::warn!("rename_preview (yaml): index refresh failed, falling back to fresh scan ({e})");
                    crate::studio::index::load(&repo_root)
                }
            };

            let kinds = [StudioFileKind::Yaml];

            let mut sites:    Vec<RenameSite> = Vec::new();
            let mut seen_key: HashSet<(String, Vec<String>)> = HashSet::new();
            for d in crate::studio::index::aggregate_cross_refs_for(&idx, &kinds) {
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
            for u in crate::studio::index::aggregate_usages_for(&idx, &old_value, &kinds) {
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
            sites.sort_by(|a, b| a.relative_path
                .cmp(&b.relative_path)
                .then_with(|| a.field_path.cmp(&b.field_path)));

            let mut file_text_cache: BTreeMap<String, String> = BTreeMap::new();
            for site in sites.iter_mut() {
                let text = file_text_cache
                    .entry(site.absolute_path.clone())
                    .or_insert_with(|| read_file_to_string(&site.absolute_path));
                site.preview = synth_preview_line(text, &site.key_name, &old_value);
            }

            let collisions = match new_value_hint.as_deref() {
                Some(hint) if !hint.is_empty() && hint != old_value => {
                    let mut out: Vec<RenameCollision> = Vec::new();
                    for d in crate::studio::index::aggregate_cross_refs_for(&idx, &kinds) {
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

        let result = tokio::task::spawn_blocking(move || -> StudioResult<RenameResult> {
            let mut by_file: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
            for s in &sites {
                by_file.entry(s.absolute_path.clone()).or_default().push(s.field_path.clone());
            }

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
                let new_text = legacy::apply_string_rename(
                    &text, &paths, &new_value,
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

            let mut written: Vec<String>        = Vec::new();
            let mut failed:  Vec<RenameFailure> = Vec::new();
            for w in pending {
                match legacy::write_to_disk(
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
                let pairs = self.lock()?.query_value_pairs(&doc_id, &query)?;
                let source_path = self.lock()?.source_path(&doc_id)?;
                let (abs_path, rel_path, file_name) = synth_active_doc_paths(&source_path);
                let mut sites: Vec<BulkEditSite> = Vec::with_capacity(pairs.len());
                for (path, value) in pairs {
                    sites.push(build_site_for_preview(
                        &abs_path, &rel_path, &file_name, &path, &value,
                        &action, &value_source, compiled.as_ref(),
                    ));
                }
                Ok(BulkEditPreview {
                    sites,
                    dirty_blockers:   Vec::new(),
                    expression_error: None,
                })
            }
            BulkEditScope::ProjectWide => {
                let query     = query.clone();
                let action    = action;
                let value_src = value_source.clone();
                let compiled  = compiled.clone();
                tokio::task::spawn_blocking(move || -> StudioResult<BulkEditPreview> {
                    let mut sites: Vec<BulkEditSite> = Vec::new();
                    let files = crate::studio::scan_repo(
                        &repo_root,
                        &[crate::studio::StudioFileKind::Yaml],
                    )?;
                    for f in &files {
                        if f.excluded { continue; }
                        let Ok(bytes) = std::fs::read(&f.absolute_path) else { continue; };
                        let (text, _enc, _had_bom) =
                            crate::git::encoding::decode_bytes_full(&bytes);
                        let Some(root) = legacy::parse_to_value(&text) else { continue; };
                        let pairs = match legacy::query_value_pairs_against(&root, &query) {
                            Ok(p)  => p,
                            Err(_) => continue,
                        };
                        for (path, pair_value) in pairs {
                            sites.push(build_site_for_preview(
                                &f.absolute_path,
                                &f.relative_path,
                                &f.name,
                                &path, &pair_value,
                                &action, &value_src, compiled.as_ref(),
                            ));
                        }
                    }
                    sites.sort_by(|a, b|
                        a.relative_path.cmp(&b.relative_path)
                            .then_with(|| a.field_path.cmp(&b.field_path))
                    );

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
                let (ops, applied, skipped) = {
                    let reg = self.lock()?;
                    let root_value = reg.query_value_pairs(&doc_id, "$")?
                        .into_iter().next()
                        .map(|(_p, v)| v)
                        .unwrap_or(serde_json::Value::Null);
                    build_ops_from_sites(
                        &root_value, &sites, &action, &value_source, compiled.as_ref(),
                    )
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
                    let mut by_file: BTreeMap<String, Vec<BulkEditSite>> = BTreeMap::new();
                    for s in sites {
                        by_file.entry(s.absolute_path.clone()).or_default().push(s);
                    }

                    struct PendingWrite {
                        abs_path:       String,
                        new_text:       String,
                        encoding_label: String,
                        had_bom:        bool,
                    }
                    let mut pending:   Vec<PendingWrite> = Vec::with_capacity(by_file.len());
                    let mut applied_n: usize             = 0;
                    let mut skipped_n: usize             = 0;
                    for (abs_path, sites_for_file) in by_file {
                        let bytes = std::fs::read(&abs_path).map_err(|e| {
                            AppError::Other(format!("Read {abs_path}: {e}"))
                        })?;
                        let (text, enc, had_bom) =
                            crate::git::encoding::decode_bytes_full(&bytes);
                        let root_value = legacy::parse_to_value(&text)
                            .ok_or_else(|| AppError::Other(format!(
                                "parse {abs_path}: invalid YAML",
                            )))?;
                        let (ops, a, s) = build_ops_from_sites(
                            &root_value, &sites_for_file, &action, &value_src, compiled.as_ref(),
                        );
                        applied_n += a;
                        skipped_n += s;
                        if ops.is_empty() { continue; }
                        let new_text = legacy::apply_bulk_edits_text(&text, &ops)
                            .map_err(|e| AppError::Other(format!(
                                "Apply edits to {abs_path}: {e}",
                            )))?;
                        pending.push(PendingWrite {
                            abs_path,
                            new_text,
                            encoding_label: enc.name().to_string(),
                            had_bom,
                        });
                    }

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

    // ── Schema panel (JSON Schema sidecar only) ──────────────────────
    //
    // YAML's descriptor declares `schema_sources = [JsonSchema]`. We
    // delegate to `crate::json_studio::schema` because the schema-shape
    // is independent of the value format — `Schema`/`TypeDef` are
    // already format-agnostic.

    async fn schema_probe(&self, source: String) -> StudioResult<CrateProbe> {
        let src = source.clone();
        let probe = tokio::task::spawn_blocking(move || -> StudioResult<CrateProbe> {
            Ok(crate::json_studio::schema::probe(&src)?)
        })
        .await
        .map_err(|e| AppError::Other(format!("schema_probe join: {e}")))??;
        Ok(probe)
    }

    async fn schema_load(
        &self,
        source:         String,
        root_canonical: String,
    ) -> StudioResult<Schema> {
        let src  = source.clone();
        let root = root_canonical.clone();
        let schema = tokio::task::spawn_blocking(move || -> StudioResult<Schema> {
            Ok(crate::json_studio::schema::load(&src, &root)?)
        })
        .await
        .map_err(|e| AppError::Other(format!("schema_load join: {e}")))??;
        Ok(schema)
    }

    async fn schema_view_source(
        &self,
        source:         String,
        canonical_path: String,
    ) -> StudioResult<TypeSource> {
        let src       = source.clone();
        let canonical = canonical_path.clone();
        let ts = tokio::task::spawn_blocking(move || -> StudioResult<TypeSource> {
            Ok(crate::json_studio::schema::get_type_source(&src, &canonical)?)
        })
        .await
        .map_err(|e| AppError::Other(format!("schema_view_source join: {e}")))??;
        Ok(ts)
    }

    // ── File listing ─────────────────────────────────────────────────

    async fn list_files(&self, folder: String) -> StudioResult<Vec<FileEntry>> {
        let entries = tokio::task::spawn_blocking(move || {
            crate::studio::scan_repo(&folder, &[crate::studio::StudioFileKind::Yaml])
        })
        .await
        .map_err(|e| AppError::Other(format!("list_files join: {e}")))??;
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
}

// ─── Conversions ──────────────────────────────────────────────────────

fn kind_to_string(k: NodeKind) -> String { k.as_str().to_string() }

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

fn update_result_into(r: legacy::UpdateResult) -> UpdateResult {
    UpdateResult {
        parse_error:        r.parse_error,
        root_kind:          r.root_kind.map(kind_to_string),
        child_count:        r.child_count,
        can_undo:           r.can_undo,
        can_redo:           r.can_redo,
        has_jsonc_features: false,
    }
}

fn mutate_result_into(r: legacy::MutateResult) -> MutateResult {
    MutateResult {
        text:               r.text,
        parse_error:        r.parse_error,
        root_kind:          r.root_kind.map(kind_to_string),
        child_count:        r.child_count,
        can_undo:           r.can_undo,
        can_redo:           r.can_redo,
        has_jsonc_features: false,
    }
}

// ─── Descriptor ───────────────────────────────────────────────────────

fn build_descriptor() -> FormatDescriptor {
    let mut kind_palette = std::collections::BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    kind_palette.insert("object".into(),  entry("object",  KindTone::Info));
    kind_palette.insert("array".into(),   entry("array",   KindTone::Info));
    kind_palette.insert("string".into(),  entry("string",  KindTone::Success));
    kind_palette.insert("integer".into(), entry("int",     KindTone::Warning));
    kind_palette.insert("float".into(),   entry("float",   KindTone::Warning));
    kind_palette.insert("bool".into(),    entry("bool",    KindTone::Warning));
    kind_palette.insert("null".into(),    entry("null",    KindTone::Muted));

    FormatDescriptor {
        id:                          "yaml".into(),
        label:                       "YAML".into(),
        file_extensions:             vec![".yaml".into(), ".yml".into()],
        icon:                        IconRef::Iconify {
            name: "vscode-icons:file-type-yaml".into(),
        },

        // 5.b — lossless edit via `yaml_edit`. The mutation path
        // preserves comments + anchors + quote style; the structural-
        // fallback path (replace/insert/duplicate/move) round-trips
        // through `serde_yaml_ng` for the affected sub-tree and may drop
        // comments around the splice site, but the rest of the doc
        // stays intact.
        supports_lossless_edit:      true,
        supports_comments:           true,
        supports_anchors:            true,
        null_handling:               NullPolicy::Native,

        supports_streaming_mode:     false,
        streaming_threshold_kb:      None,
        streaming_setting_key:       None,

        query_syntax:                QuerySyntax::JsonPath,

        cross_ref_default_fields:    vec!["id".into(), "name".into()],
        cross_ref_scopes:            vec![CrossRefScope::Value],

        schema_sources:              vec![SchemaSourceKind::JsonSchema],

        kind_palette,

        // 5.b — lossless save: drop the legacy LossyComments warning.
        // The descriptor still tells the FE which warnings to render,
        // and `save_warnings = []` keeps it format-agnostic.
        save_warnings:               vec![],
        save_behavior_setting_key:   None,

        convert_to_json_supported:   true,

        supports_external_files:     true,

        // Phase 5.c — F12 rename + F13 bulk edit + schema panel lit up.
        supports_rename_reference:   true,
        supports_bulk_edit:          true,
    }
}

// ─── F13 helpers ─────────────────────────────────────────────────────

fn synth_active_doc_paths(source_path: &Option<String>) -> (String, String, String) {
    match source_path {
        Some(p) => {
            let norm = p.replace('\\', "/");
            let name = norm.rsplit('/').next().unwrap_or(&norm).to_string();
            (p.clone(), norm, name)
        }
        None => (
            "(active doc)".to_string(),
            "(active doc)".to_string(),
            "(active doc)".to_string(),
        ),
    }
}

fn build_site_for_preview(
    abs_path:     &str,
    rel_path:     &str,
    file_name:    &str,
    field_path:   &[String],
    target:       &serde_json::Value,
    action:       &BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> BulkEditSite {
    let kind        = legacy::yaml_kind_str(target).to_string();
    let old_preview = legacy::yaml_preview_for(target);
    let mut will_skip   = false;
    let mut skip_reason = String::new();
    let mut new_preview = String::new();

    let is_container = matches!(target, serde_json::Value::Object(_) | serde_json::Value::Array(_));

    match action {
        BulkEditAction::Delete => {
            if field_path.is_empty() {
                will_skip = true;
                skip_reason = "Cannot delete the document root".into();
            } else {
                new_preview = "(removed)".into();
            }
        }
        BulkEditAction::Set => {
            if is_container {
                will_skip = true;
                skip_reason = "`set` cannot target a container node — descend deeper into the query".into();
            } else {
                match compute_new_value(target, value_source, compiled) {
                    Ok(v)        => new_preview = render_set_preview(&v),
                    Err(reason)  => {
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
        old_preview,
        new_preview,
        will_skip,
        skip_reason,
    }
}

/// Resolve a value-source against `target` and produce a `YamlSetValue`.
/// YAML descriptor declares `null_handling = Native` so `null` stays a
/// literal write (unlike TOML which routes null → delete).
fn compute_new_value(
    target:       &serde_json::Value,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> Result<YamlSetValue, String> {
    let raw_value: ExprValue = match value_source {
        Some(BulkEditValueSource::Literal { literal }) => match literal {
            BulkEditLiteral::String(s) => ExprValue::String(s.clone()),
            BulkEditLiteral::Number(n) => ExprValue::Number(*n),
            BulkEditLiteral::Bool(b)   => ExprValue::Bool(*b),
            BulkEditLiteral::Null      => ExprValue::Null,
        },
        Some(BulkEditValueSource::Expression { .. }) => {
            let compiled = compiled.ok_or_else(|| "internal: compiled expression missing".to_string())?;
            let old = json_to_eval_value(target)
                .ok_or_else(|| "container node — cannot bind `old`".to_string())?;
            match compiled.eval(&old) {
                Ok(v) => v,
                Err(e) => return Err(e.0),
            }
        }
        None => return Err("Value source missing for `set` action".into()),
    };

    Ok(match raw_value {
        ExprValue::Null      => YamlSetValue::Null,
        ExprValue::Bool(b)   => YamlSetValue::Bool(b),
        ExprValue::Number(n) => {
            // Match YAML's typed-number distinction the same way TOML
            // does: prefer integer when integral & in i64 range; float
            // otherwise. Both survive the lossless `set_path` route.
            if n.is_finite() && n.fract() == 0.0 && n.abs() < (i64::MAX as f64) {
                YamlSetValue::Integer(n as i64)
            } else {
                YamlSetValue::Float(n)
            }
        }
        ExprValue::String(s) => YamlSetValue::String(s),
    })
}

fn json_to_eval_value(v: &serde_json::Value) -> Option<ExprValue> {
    match v {
        serde_json::Value::Null      => Some(ExprValue::Null),
        serde_json::Value::Bool(b)   => Some(ExprValue::Bool(*b)),
        serde_json::Value::Number(n) => n.as_f64().map(ExprValue::Number),
        serde_json::Value::String(s) => Some(ExprValue::String(s.clone())),
        _ => None,
    }
}

fn render_set_preview(v: &YamlSetValue) -> String {
    match v {
        YamlSetValue::String(s)  => format!("\"{s}\""),
        YamlSetValue::Integer(i) => i.to_string(),
        YamlSetValue::Float(f)   => f.to_string(),
        YamlSetValue::Bool(b)    => b.to_string(),
        YamlSetValue::Null       => "null".into(),
    }
}

fn build_ops_from_sites(
    root_value:   &serde_json::Value,
    sites:        &[BulkEditSite],
    action:       &BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> (Vec<(Vec<String>, YamlBulkOp)>, usize, usize) {
    let mut ops:     Vec<(Vec<String>, YamlBulkOp)> = Vec::with_capacity(sites.len());
    let mut applied: usize = 0;
    let mut skipped: usize = 0;
    for site in sites {
        if site.will_skip { skipped += 1; continue; }
        let Some(target) = resolve_value_path(root_value, &site.field_path) else {
            skipped += 1;
            continue;
        };
        match action {
            BulkEditAction::Delete => {
                if site.field_path.is_empty() { skipped += 1; continue; }
                ops.push((site.field_path.clone(), YamlBulkOp::Delete));
                applied += 1;
            }
            BulkEditAction::Set => {
                match compute_new_value(target, value_source, compiled) {
                    Ok(v) => {
                        ops.push((site.field_path.clone(), YamlBulkOp::Set(v)));
                        applied += 1;
                    }
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    (ops, applied, skipped)
}

fn resolve_value_path<'a>(
    root: &'a serde_json::Value,
    path: &[String],
) -> Option<&'a serde_json::Value> {
    let mut cur = root;
    for seg in path {
        cur = match cur {
            serde_json::Value::Object(m) => m.get(seg)?,
            serde_json::Value::Array(a)  => {
                let i: usize = seg.parse().ok()?;
                a.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

// ─── F12 helpers ─────────────────────────────────────────────────────

fn read_file_to_string(abs_path: &str) -> String {
    let Ok(bytes) = std::fs::read(abs_path) else { return String::new(); };
    let (text, _, _) = crate::git::encoding::decode_bytes_full(&bytes);
    text
}

/// YAML-aware preview line — match on `key:` + the value text on the
/// same line. YAML allows both `key: "value"` (quoted) and `key: value`
/// (unquoted), so we look for either shape. Falls back to a value-only
/// match when key+value sit on separate lines (block-scalar style).
fn synth_preview_line(text: &str, key: &str, value: &str) -> String {
    if text.is_empty() { return String::new(); }
    let needle_quoted   = format!("\"{value}\"");
    let needle_singleq  = format!("'{value}'");
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() { continue; }
        let has_key = l.starts_with(key) || l.starts_with(&format!("- {key}"))
            || l.contains(&format!("{key}:"));
        let has_val = l.contains(&needle_quoted)
            || l.contains(&needle_singleq)
            || l.contains(value);
        if has_key && has_val {
            best = Some(l);
            break;
        }
        if best.is_none() && has_val { best = Some(l); }
    }
    let line = best.unwrap_or("").to_string();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

fn canonicalise_path_key(p: &str) -> String {
    p.replace('\\', "/").to_ascii_lowercase()
}
