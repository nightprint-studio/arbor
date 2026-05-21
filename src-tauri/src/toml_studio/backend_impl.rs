//! `TomlBackend` — `StudioFormatBackend` implementation for TOML
//! (Phase 4.a + 4.b: read-only navigation + lossless edit via `toml_edit`).
//!
//! Mutations clone the `toml_edit::DocumentMut`, mutate the clone, and
//! re-serialise — `toml_edit` natively preserves comments / whitespace /
//! ordering / inline-vs-table formatting, so we don't need byte-splice
//! machinery like JSON Studio does. Save round-trips through
//! `git::encoding::encode_for_disk_with_bom` per FROZEN F16.
//!
//! What's still gated off for later sub-phases:
//!   - Cross-refs / F12 rename / F13 bulk edit / schema panel (Phase 4.c).

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::error::AppError;
use crate::toml_studio::{
    self as legacy, NodeKind, TomlBulkOp, TomlSetValue, TomlStudioRegistry,
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

pub struct TomlBackend {
    regs:       Mutex<TomlStudioRegistry>,
    descriptor: FormatDescriptor,
}

impl TomlBackend {
    pub fn new() -> Self {
        Self {
            regs:       Mutex::new(TomlStudioRegistry::default()),
            descriptor: build_descriptor(),
        }
    }

    fn lock(&self) -> StudioResult<std::sync::MutexGuard<'_, TomlStudioRegistry>> {
        self.regs
            .lock()
            .map_err(|_| StudioError::App(AppError::Other("toml_studio registry poisoned".into())))
    }
}

impl Default for TomlBackend {
    fn default() -> Self { Self::new() }
}

/// Public factory used by `lib.rs::run()` to populate the registry.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    Arc::new(TomlBackend::new())
}

#[async_trait]
impl StudioFormatBackend for TomlBackend {
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
            // TOML has no Option/None — descriptor declares
            // `null_handling = AsDelete`, so the FE never offers
            // toggle-option for this format. Belt-and-braces.
            StudioMutation::ToggleOption { .. } => {
                return Err(StudioError::unsupported("toml", "toggle_option"));
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

    // ── F12 — Rename refactor (lossless via toml_edit) ───────────────

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
            // Refresh the TOML slice of the index. Falls back to whatever's
            // already on disk if refresh fails.
            let idx = match crate::studio::index::refresh_for(
                &repo_root,
                &[StudioFileKind::Toml],
                None,
            ) {
                Ok(i)  => i,
                Err(e) => {
                    tracing::warn!("rename_preview (toml): index refresh failed, falling back to fresh scan ({e})");
                    crate::studio::index::load(&repo_root)
                }
            };

            let kinds = [StudioFileKind::Toml];

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

            // Best-effort line-snippet preview.
            let mut file_text_cache: BTreeMap<String, String> = BTreeMap::new();
            for site in sites.iter_mut() {
                let text = file_text_cache
                    .entry(site.absolute_path.clone())
                    .or_insert_with(|| read_file_to_string(&site.absolute_path));
                site.preview = synth_preview_line(text, &site.key_name, &old_value);
            }

            // Collisions — defs whose value equals the new hint.
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

            // Dirty blockers.
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

        // Defensive dirty re-check.
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
            // Group sites by absolute_path so each file is parsed once.
            let mut by_file: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
            for s in &sites {
                by_file.entry(s.absolute_path.clone()).or_default().push(s.field_path.clone());
            }

            // Phase A — parse + apply in memory. Any failure aborts
            // BEFORE any disk write (FROZEN F12). Each file remembers
            // its own encoding so the flush phase re-encodes per-file.
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
                let new_text = crate::toml_studio::apply_string_rename(
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

            // Phase B — flush sequentially.
            let mut written: Vec<String>        = Vec::new();
            let mut failed:  Vec<RenameFailure> = Vec::new();
            for w in pending {
                match crate::toml_studio::write_to_disk(
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
        // Compile up-front so the per-site loop doesn't pay it.
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
                        &[crate::studio::StudioFileKind::Toml],
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
                                "parse {abs_path}: invalid TOML",
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

    // ── File listing ─────────────────────────────────────────────────

    async fn list_files(&self, folder: String) -> StudioResult<Vec<FileEntry>> {
        let entries = tokio::task::spawn_blocking(move || {
            crate::studio::scan_repo(&folder, &[crate::studio::StudioFileKind::Toml])
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

    // ── Schema panel (RustStruct + JsonSchema, dispatch by extension) ─
    //
    // TOML's descriptor declares both schema sources. The trait IPC
    // hands us a single `source` path — we sniff `.rs` vs everything-else
    // to route to the right schema walker. Both return the same shared
    // `Schema`/`CrateProbe`/`TypeSource` shapes (defined in
    // `ron_studio::schema`), so the FE consumes them uniformly.

    async fn schema_probe(&self, source: String) -> StudioResult<CrateProbe> {
        let src = source.clone();
        let probe = tokio::task::spawn_blocking(move || -> StudioResult<CrateProbe> {
            if schema_source_is_rust(&src) {
                Ok(crate::ron_studio::schema::probe(&src)?)
            } else {
                Ok(crate::json_studio::schema::probe(&src)?)
            }
        })
        .await
        .map_err(|e| AppError::Other(format!("schema_probe join: {e}")))??;
        Ok(probe)
    }

    async fn schema_load(
        &self,
        source:          String,
        root_canonical: String,
    ) -> StudioResult<Schema> {
        let src  = source.clone();
        let root = root_canonical.clone();
        let schema = tokio::task::spawn_blocking(move || -> StudioResult<Schema> {
            if schema_source_is_rust(&src) {
                Ok(crate::ron_studio::schema::load(&src, &root)?)
            } else {
                Ok(crate::json_studio::schema::load(&src, &root)?)
            }
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
            if schema_source_is_rust(&src) {
                Ok(crate::ron_studio::schema::get_type_source(&src, &canonical)?)
            } else {
                Ok(crate::json_studio::schema::get_type_source(&src, &canonical)?)
            }
        })
        .await
        .map_err(|e| AppError::Other(format!("schema_view_source join: {e}")))??;
        Ok(ts)
    }
}

/// Route a schema-source path to the right walker. `.rs` → Rust crate
/// (syn + crate-walking from `Cargo.toml`); everything else is treated
/// as a JSON Schema file (`*.schema.json`, `*.json` with a `$schema`
/// keyword, …). The dispatch is filename-only — we don't open the file
/// to sniff `$schema` because the JSON Schema walker already errors
/// cleanly when the file isn't valid JSON.
fn schema_source_is_rust(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".rs")
}

// ─── Descriptor ───────────────────────────────────────────────────────

fn build_descriptor() -> FormatDescriptor {
    let mut kind_palette = BTreeMap::new();
    let entry = |label: &str, tone: KindTone| KindStyle {
        label: label.to_string(),
        tone,
        icon: None,
    };
    // FROZEN F11 — TOML kinds stay distinct (table vs inline_table vs
    // array_of_tables, etc.). The FE renders each one with its own chip.
    kind_palette.insert("table".into(),           entry("table",     KindTone::Info));
    kind_palette.insert("inline_table".into(),    entry("inline",    KindTone::Info));
    kind_palette.insert("array".into(),           entry("array",     KindTone::Info));
    kind_palette.insert("array_of_tables".into(), entry("array<table>", KindTone::Accent));
    kind_palette.insert("string".into(),          entry("string",    KindTone::Success));
    kind_palette.insert("integer".into(),         entry("int",       KindTone::Warning));
    kind_palette.insert("float".into(),           entry("float",     KindTone::Warning));
    kind_palette.insert("bool".into(),            entry("bool",      KindTone::Warning));
    kind_palette.insert("datetime".into(),        entry("datetime",  KindTone::Accent));

    FormatDescriptor {
        id:                          "toml".into(),
        label:                       "TOML".into(),
        file_extensions:             vec![".toml".into()],
        icon:                        IconRef::Iconify {
            name: "vscode-icons:file-type-toml".into(),
        },

        // Phase 4.b lights this up; `toml_edit` preserves formatting
        // natively so mutations round-trip losslessly.
        supports_lossless_edit:      true,
        supports_comments:           true,
        supports_anchors:            false,
        // FROZEN F13 — TOML has no native null; bulk-edit `null` →
        // remove key. The descriptor drives the modal banner so the FE
        // doesn't branch on `format_id`.
        null_handling:               NullPolicy::AsDelete,

        // Streaming mode: not wired yet. TOML files rarely run into
        // megabyte territory in practice; if they do we can add a
        // threshold here.
        supports_streaming_mode:     false,
        streaming_threshold_kb:      None,
        streaming_setting_key:       None,

        query_syntax:                QuerySyntax::JsonPath,

        // Default convention — same as RON / JSON. 4.c will allow
        // custom patterns via `.studio.toml` bindings.
        cross_ref_default_fields:    vec!["id".into(), "name".into()],
        cross_ref_scopes:            vec![CrossRefScope::Value],

        schema_sources:              vec![
            SchemaSourceKind::RustStruct,
            SchemaSourceKind::JsonSchema,
        ],

        kind_palette,

        save_warnings:               Vec::new(),
        save_behavior_setting_key:   None,

        // TOML projection to JSON is meaningful (used by the schema
        // panel + cross-format conversions); flag it for the FE menu.
        convert_to_json_supported:   true,

        supports_external_files:     true,

        // Phase 4.c.a: F12 rename lit up.
        // Phase 4.c.b.1: F13 bulk edit lit up.
        supports_rename_reference:   true,
        supports_bulk_edit:          true,
    }
}

// ─── F13 helpers ────────────────────────────────────────────────────

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
    let kind        = legacy::toml_kind_str(target).to_string();
    let old_preview = legacy::toml_preview_for(target);
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
                    Ok(SetOutcome::Set(v))   => new_preview = render_set_preview(&v),
                    Ok(SetOutcome::DeleteViaNull) => {
                        // FROZEN F13 — `null_handling = AsDelete`. The
                        // user typed `null` (literal or via expression)
                        // on a TOML site, which has no native null.
                        if field_path.is_empty() {
                            will_skip = true;
                            skip_reason = "Cannot delete the document root".into();
                        } else {
                            new_preview = "(removed via null)".into();
                        }
                    }
                    Err(reason)              => {
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

/// Per-site outcome of value-source resolution. TOML's `null_handling
/// = AsDelete` rules turn a null payload into a delete op at apply
/// time; this enum lifts that signalling out of the value enum so the
/// caller can dispatch to the right op kind.
enum SetOutcome {
    Set(TomlSetValue),
    DeleteViaNull,
}

fn compute_new_value(
    target:       &serde_json::Value,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> Result<SetOutcome, String> {
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
        ExprValue::Null      => SetOutcome::DeleteViaNull,
        ExprValue::Bool(b)   => SetOutcome::Set(TomlSetValue::Bool(b)),
        ExprValue::Number(n) => {
            // Integer when integral within i64 range; float otherwise.
            // Mirrors TOML's typed-number distinction.
            if n.is_finite() && n.fract() == 0.0 && n.abs() < (i64::MAX as f64) {
                SetOutcome::Set(TomlSetValue::Integer(n as i64))
            } else {
                SetOutcome::Set(TomlSetValue::Float(n))
            }
        }
        ExprValue::String(s) => SetOutcome::Set(TomlSetValue::String(s)),
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

fn render_set_preview(v: &TomlSetValue) -> String {
    match v {
        TomlSetValue::String(s)  => format!("\"{s}\""),
        TomlSetValue::Integer(i) => i.to_string(),
        TomlSetValue::Float(f)   => f.to_string(),
        TomlSetValue::Bool(b)    => b.to_string(),
    }
}

fn build_ops_from_sites(
    root_value:   &serde_json::Value,
    sites:        &[BulkEditSite],
    action:       &BulkEditAction,
    value_source: &Option<BulkEditValueSource>,
    compiled:     Option<&CompiledExpr>,
) -> (Vec<(Vec<String>, TomlBulkOp)>, usize, usize) {
    let mut ops:     Vec<(Vec<String>, TomlBulkOp)> = Vec::with_capacity(sites.len());
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
                ops.push((site.field_path.clone(), TomlBulkOp::Delete));
                applied += 1;
            }
            BulkEditAction::Set => {
                match compute_new_value(target, value_source, compiled) {
                    Ok(SetOutcome::Set(v)) => {
                        ops.push((site.field_path.clone(), TomlBulkOp::Set(v)));
                        applied += 1;
                    }
                    Ok(SetOutcome::DeleteViaNull) => {
                        if site.field_path.is_empty() { skipped += 1; continue; }
                        ops.push((site.field_path.clone(), TomlBulkOp::Delete));
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

/// TOML-aware preview line — TOML keys live on their own line as
/// `key = "value"`. Look for a line that mentions both the key and
/// the quoted value; fall back to a value-only match (handy when the
/// key was promoted to a `[section]` header rather than an inline
/// assignment).
fn synth_preview_line(text: &str, key: &str, value: &str) -> String {
    if text.is_empty() { return String::new(); }
    let needle_val = format!("\"{value}\"");
    let mut best: Option<&str> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() { continue; }
        // The key on the LHS of `=` (potentially quoted) — cheap match.
        let has_key = l.starts_with(key) || l.contains(&format!("\"{key}\""));
        if has_key && l.contains(&needle_val) {
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

fn canonicalise_path_key(p: &str) -> String {
    p.replace('\\', "/").to_ascii_lowercase()
}

// ─── Type conversions ────────────────────────────────────────────────

fn kind_to_str(k: NodeKind) -> &'static str { k.as_str() }
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
        original:    String::new(),
        parse_error: p.parse_error,
        root_kind:   p.root_kind.map(kind_to_string),
        child_count: p.child_count,
        schema_hint: None,
        encoding,
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
