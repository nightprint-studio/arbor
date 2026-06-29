//! `format` domain — the format-agnostic Studio editor surface (RON / JSON /
//! TOML / YAML / .properties) dispatched through `AppState.studio_registry`.
//!
//! Each handler is the body the matching `#[tauri::command] fn` ran, now a plain
//! function self-registered under `program = "studio"`. Every call takes
//! `format_id` as its first JSON argument and resolves the backend via the
//! registry; `StudioError` is mapped to a wire string through `errors::to_ipc`.
//!
//! **Synchronous** handlers (`fn`) run on `spawn_blocking` via the `rpc`
//! command. **Async** handlers (`async fn`) are awaited on the runtime via
//! `dispatch_async` — they register as `Kind::Async` in the `arbor-rpc`
//! inventory and are served by `crate::ipc::is_async_method` /
//! `crate::ipc::dispatch_async`. Both paths share the same
//! `#[studio::handler(program = "studio")]` attribute.

use crate::error::AppError;
use crate::ipc::studio;
use crate::studio::format::descriptor::FormatDescriptor;
use crate::studio::format::errors::to_ipc;
use crate::studio::format::properties_codec::{
    self, PropertiesToYamlOptions, PropertiesToYamlOutput, YamlToPropertiesOutput,
};
use crate::studio::format::types::{
    BulkEditAction, BulkEditOpenDoc, BulkEditPreview, BulkEditResult, BulkEditScope,
    BulkEditSite, BulkEditValueSource, CrateProbe, DiffHunk, DiffTreeNode, DocSnapshot,
    EncodingInfo, FileEntry, MutateResult, NodeView, ParseResult, QueryHit, RenameOpenDoc,
    RenamePreview, RenameResult, RenameSite, Schema, SchemaHint, SchemaHintOrigin, StudioMutation,
    TypeSource, UpdateResult,
};
use crate::AppState;

// ── Descriptor introspection ─────────────────────────────────────────────────

/// List every registered format backend's descriptor. Bare-value command in the
/// old surface; wrapped in `Result` here because the handler macro requires a
/// `Result<R, E>` shape (the error arm is never produced).
#[studio::handler(program = "studio")]
fn studio_list_formats(state: &AppState) -> Result<Vec<FormatDescriptor>, AppError> {
    Ok(state.studio_registry.list_descriptors())
}

#[studio::handler(program = "studio")]
fn studio_describe(state: &AppState, format_id: String) -> Result<FormatDescriptor, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    Ok(backend.descriptor().clone())
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_close(state: &AppState, format_id: String, doc_id: String) -> Result<(), String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.close(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_get_encoding(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<EncodingInfo, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.get_encoding(&doc_id))
}

// ── Text & raw access ────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_set_text(
    state: &AppState,
    format_id: String,
    doc_id: String,
    text: String,
) -> Result<UpdateResult, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.set_text(&doc_id, text))
}

#[studio::handler(program = "studio")]
fn studio_raw_original(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.raw_original(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_raw_current(state: &AppState, format_id: String, doc_id: String) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.raw_current(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_format(state: &AppState, format_id: String, doc_id: String) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.format_doc(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_to_json(state: &AppState, format_id: String, doc_id: String) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.to_json(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_from_json(
    state: &AppState,
    format_id: String,
    doc_id: String,
    json_text: String,
) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.from_json(&doc_id, json_text))
}

#[studio::handler(program = "studio")]
fn studio_get_indent(state: &AppState, format_id: String, doc_id: String) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.get_indent(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_set_indent(
    state: &AppState,
    format_id: String,
    doc_id: String,
    indent: String,
) -> Result<(), String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.set_indent(&doc_id, indent))
}

// ── Tree navigation ──────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_get_root(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<Option<NodeView>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.get_root(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_get_children(
    state: &AppState,
    format_id: String,
    doc_id: String,
    path: Vec<String>,
) -> Result<Vec<NodeView>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.get_children(&doc_id, path))
}

#[studio::handler(program = "studio")]
fn studio_get_value(
    state: &AppState,
    format_id: String,
    doc_id: String,
    path: Vec<String>,
) -> Result<String, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.get_value(&doc_id, path))
}

// ── Query ────────────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_query(
    state: &AppState,
    format_id: String,
    doc_id: String,
    expr: String,
) -> Result<Vec<QueryHit>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.query(&doc_id, expr))
}

// ── Mutations ────────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_apply_mutation(
    state: &AppState,
    format_id: String,
    doc_id: String,
    mutation: StudioMutation,
) -> Result<MutateResult, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.apply_mutation(&doc_id, mutation))
}

/// Phase 3.d — re-emit the doc lossy-stripping format-specific extras
/// (JSON Studio: comments + trailing commas). Backends that don't
/// support the operation return `Unsupported`; the FE checks
/// `descriptor.save_warnings` to know when to offer the action.
#[studio::handler(program = "studio")]
fn studio_strip_features(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<MutateResult, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.strip_features(&doc_id))
}

// ── Diff ─────────────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_diff(state: &AppState, format_id: String, doc_id: String) -> Result<Vec<DiffHunk>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.diff(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_tree_diff(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<DiffTreeNode, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.tree_diff(&doc_id))
}

// ── History ──────────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_undo(state: &AppState, format_id: String, doc_id: String) -> Result<MutateResult, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.undo(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_redo(state: &AppState, format_id: String, doc_id: String) -> Result<MutateResult, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.redo(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_history_state(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<(bool, bool), String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.history_state(&doc_id))
}

// ── Snapshot & persistence ───────────────────────────────────────────────────

#[studio::handler(program = "studio")]
fn studio_snapshot(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<DocSnapshot, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.snapshot(&doc_id))
}

#[studio::handler(program = "studio")]
fn studio_source_path(
    state: &AppState,
    format_id: String,
    doc_id: String,
) -> Result<Option<String>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.source_path(&doc_id))
}

// ── YAML ↔ .properties converter (Phase 5.b extension) ───────────────────────
//
// Cross-format codec exposed through dedicated commands rather than
// the per-format `StudioFormatBackend` trait — the conversion is
// neither a "YAML operation" nor a ".properties operation", it's a
// bridge between the two. Lives in `studio::format::properties_codec`
// so Phase 6 (.properties Studio) reuses the same engine.

// The codec commands carry no real state, but the handler macro mandates a
// `&Ctx` first param — `_state` satisfies it without altering the wire args
// (the context is passed type-erased, never serialized).
#[studio::handler(program = "studio")]
fn studio_yaml_to_properties(
    _state: &AppState,
    text: String,
) -> Result<YamlToPropertiesOutput, String> {
    properties_codec::yaml_to_properties(&text).map_err(|e| e.to_string())
}

#[studio::handler(program = "studio")]
fn studio_properties_to_yaml(
    _state: &AppState,
    text: String,
    strings_only: Option<bool>,
) -> Result<PropertiesToYamlOutput, String> {
    let opts = PropertiesToYamlOptions {
        strings_only: strings_only.unwrap_or(false),
    };
    properties_codec::properties_to_yaml(&text, &opts).map_err(|e| e.to_string())
}

// ── Async handlers (awaited on the runtime, not spawn_blocking) ──────────────
//
// These were kept as `#[tauri::command] async fn`s in
// `studio::format::commands` while the macro was sync-only. Now that
// `#[studio::handler]` supports `async fn` (registers `Kind::Async`,
// served via `crate::ipc::dispatch_async`), they move here.

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Parse a studio document. Reads from `path` when `text` is not
/// provided, decoding raw bytes through `git::encoding::decode_bytes_full`
/// so legacy files (windows-1252, UTF-16 BOM) survive a round-trip.
/// A cfg-keyed `schema_hint` fallback fires when the inline detection
/// found nothing and a `tab_id` + `relative_path` context is available.
#[studio::handler(program = "studio")]
async fn studio_parse(
    state: &AppState,
    format_id: String,
    text: Option<String>,
    path: Option<String>,
    tab_id: Option<String>,
    relative_path: Option<String>,
) -> Result<ParseResult, String> {
    // FROZEN F16: never use `read_to_string` here. Read raw bytes and
    // pass through `git::encoding::decode_bytes_full` so legacy files
    // (windows-1252, UTF-16 BOM) survive an edit/save round-trip. The
    // sniffed encoding label propagates into the backend doc state and
    // is replayed at save time via `encode_for_disk_with_bom`.
    let (text, source_path, encoding) = match (text, path) {
        (Some(t), p)    => (t, p, EncodingInfo::utf8()),
        (None, Some(p)) => {
            let bytes = std::fs::read(&p)
                .map_err(|e| format!("Cannot read {p}: {e}"))?;
            let (content, enc, had_bom) =
                crate::git::encoding::decode_bytes_full(&bytes);
            let info = EncodingInfo {
                label:   enc.name().to_string(),
                had_bom,
            };
            (content, Some(p), info)
        }
        (None, None) => return Err("studio_parse: provide `text` or `path`".into()),
    };

    // Resolve the repo path up-front so we can release the repo lock
    // before dispatching to the backend.
    let repo_path = match tab_id.as_deref() {
        Some(t) => state
            .lock_repos()
            .ok()
            .and_then(|mut m| m.get(t).ok().map(|r| r.path.clone())),
        None => None,
    };

    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    let mut result = to_ipc(backend.parse(text, source_path, encoding).await)?;

    // Format-agnostic cfg-keyed schema_hint fallback: when the
    // backend's inline detection found nothing AND we have a
    // tab + relative-path context, try the side-car binding. Covers
    // external files whose disk path sits outside the repo tree.
    if result.schema_hint.is_none() {
        if let (Some(repo), Some(rel)) = (repo_path, relative_path) {
            let cfg = crate::studio::config::load(&repo).unwrap_or_default();
            if let Some((rs_file, root_type)) =
                crate::studio::config::resolve_binding(&cfg, &repo, &rel)
            {
                result.schema_hint = Some(SchemaHint {
                    rs_file,
                    root_type,
                    origin: SchemaHintOrigin::Sidecar,
                });
            }
        }
    }

    Ok(result)
}

// ── Snapshot & persistence ───────────────────────────────────────────────────

#[studio::handler(program = "studio")]
async fn studio_save(
    state: &AppState,
    format_id: String,
    doc_id: String,
    path: String,
    contents: String,
    bind_to_doc: bool,
) -> Result<(), String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.save(&doc_id, path, contents, bind_to_doc).await)
}

// ── File listing ─────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
async fn studio_list_files(
    state: &AppState,
    format_id: String,
    folder: String,
) -> Result<Vec<FileEntry>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.list_files(folder).await)
}

// ── Schema ───────────────────────────────────────────────────────────────────

#[studio::handler(program = "studio")]
async fn studio_schema_probe(
    state: &AppState,
    format_id: String,
    source: String,
) -> Result<CrateProbe, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.schema_probe(source).await)
}

#[studio::handler(program = "studio")]
async fn studio_schema_load(
    state: &AppState,
    format_id: String,
    source: String,
    root_canonical: String,
) -> Result<Schema, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.schema_load(source, root_canonical).await)
}

#[studio::handler(program = "studio")]
async fn studio_schema_view_source(
    state: &AppState,
    format_id: String,
    source: String,
    canonical_path: String,
) -> Result<TypeSource, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.schema_view_source(source, canonical_path).await)
}

// ── F12 — Cross-reference rename refactor ────────────────────────────────────
//
// `tab_id` lets the FE pass an active-tab handle instead of resolving
// the repo root client-side: the BE looks up the path via the same
// `lock_repos()` registry every other studio command uses. Hard error
// when the tab is unknown — refactoring against an unregistered tab
// has no defined semantics (the `repo_root`-driven scan needs a real
// project root).

/// Preview the rename across the active tab's repo. Returns the full
/// site list (defs + refs), any `new_value` collisions, and any open
/// docs whose unsaved state would block the apply step.
#[studio::handler(program = "studio")]
async fn studio_rename_preview(
    state: &AppState,
    format_id: String,
    tab_id: String,
    old_value: String,
    new_value_hint: Option<String>,
    open_docs: Vec<RenameOpenDoc>,
) -> Result<RenamePreview, String> {
    let repo_path = {
        let mut mgr = state.lock_repos().map_err(|e| e.to_string())?;
        mgr.get(&tab_id).map_err(|e| e.to_string())?.path.clone()
    };
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.rename_preview(repo_path, old_value, new_value_hint, open_docs).await)
}

/// Apply the rename. The FE sends back the (possibly user-pruned)
/// site list from the preview step. Best-effort sequential with
/// rollback PRE-flush — see `StudioFormatBackend::rename_apply`.
#[studio::handler(program = "studio")]
async fn studio_rename_apply(
    state: &AppState,
    format_id: String,
    tab_id: String,
    old_value: String,
    new_value: String,
    sites: Vec<RenameSite>,
    open_docs: Vec<RenameOpenDoc>,
) -> Result<RenameResult, String> {
    let repo_path = {
        let mut mgr = state.lock_repos().map_err(|e| e.to_string())?;
        mgr.get(&tab_id).map_err(|e| e.to_string())?.path.clone()
    };
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.rename_apply(repo_path, old_value, new_value, sites, open_docs).await)
}

// ── F13 — Query-driven bulk edit ─────────────────────────────────────
//
// `tab_id` resolves the repo root for the `ProjectWide` scope (same
// pattern as the rename commands). `doc_id` identifies the active
// doc — required for `ActiveDoc` scope, ignored for `ProjectWide`.
// `value_source` is `None` for `Action::Delete` and `Some(...)` for
// `Action::Set`. Compile errors in the mini-expression land in the
// `expression_error` field of the preview, NOT in the result Err.

// Args mirror the IPC payload; the handler macro deserializes each field.
#[allow(clippy::too_many_arguments)]
#[studio::handler(program = "studio")]
async fn studio_bulk_edit_preview(
    state: &AppState,
    format_id: String,
    tab_id: String,
    doc_id: String,
    scope: BulkEditScope,
    query: String,
    action: BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    open_docs: Vec<BulkEditOpenDoc>,
) -> Result<BulkEditPreview, String> {
    let repo_path = {
        let mut mgr = state.lock_repos().map_err(|e| e.to_string())?;
        mgr.get(&tab_id).map_err(|e| e.to_string())?.path.clone()
    };
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.bulk_edit_preview(
        repo_path, doc_id, scope, query, action, value_source, open_docs,
    ).await)
}

#[allow(clippy::too_many_arguments)]
#[studio::handler(program = "studio")]
async fn studio_bulk_edit_apply(
    state: &AppState,
    format_id: String,
    tab_id: String,
    doc_id: String,
    scope: BulkEditScope,
    action: BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    sites: Vec<BulkEditSite>,
    open_docs: Vec<BulkEditOpenDoc>,
) -> Result<BulkEditResult, String> {
    let repo_path = {
        let mut mgr = state.lock_repos().map_err(|e| e.to_string())?;
        mgr.get(&tab_id).map_err(|e| e.to_string())?.path.clone()
    };
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.bulk_edit_apply(
        repo_path, doc_id, scope, action, value_source, sites, open_docs,
    ).await)
}
