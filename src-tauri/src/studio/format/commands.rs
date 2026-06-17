//! Format-agnostic Tauri commands — the **genuinely-async** slice.
//!
//! Every command takes `format_id` as its first parameter and
//! dispatches via `AppState.studio_registry`. The Tauri layer returns
//! `Result<T, String>` — `StudioError` is mapped via
//! `errors::to_ipc` to keep the frontend free of enum boilerplate.
//!
//! Only the commands that `.await` a real backend future live here. The
//! synchronous slice (descriptor introspection, text/raw access, tree
//! navigation, query, mutations, diff, history, snapshot, the
//! YAML↔.properties codec) migrated to the Model-D `studio` backend at
//! `crate::ipc::studio::format`. These stay inline as keep-shell Tauri
//! commands because the `#[studio::handler]` macro is sync-only.

use tauri::State;

use crate::AppState;

use super::errors::to_ipc;
use super::types::{
    BulkEditAction, BulkEditOpenDoc, BulkEditPreview, BulkEditResult,
    BulkEditScope, BulkEditSite, BulkEditValueSource, CrateProbe, EncodingInfo,
    FileEntry, ParseResult, RenameOpenDoc, RenamePreview, RenameResult,
    RenameSite, Schema, SchemaHint, SchemaHintOrigin, TypeSource,
};

// ── Lifecycle ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn studio_parse(
    state:         State<'_, AppState>,
    format_id:     String,
    text:          Option<String>,
    path:          Option<String>,
    tab_id:        Option<String>,
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

#[tauri::command]
pub async fn studio_save(
    state:        State<'_, AppState>,
    format_id:    String,
    doc_id:       String,
    path:         String,
    contents:     String,
    bind_to_doc:  bool,
) -> Result<(), String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.save(&doc_id, path, contents, bind_to_doc).await)
}

// ── File listing ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn studio_list_files(
    state:     State<'_, AppState>,
    format_id: String,
    folder:    String,
) -> Result<Vec<FileEntry>, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.list_files(folder).await)
}

// ── Schema ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn studio_schema_probe(
    state:     State<'_, AppState>,
    format_id: String,
    source:    String,
) -> Result<CrateProbe, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.schema_probe(source).await)
}

#[tauri::command]
pub async fn studio_schema_load(
    state:          State<'_, AppState>,
    format_id:      String,
    source:         String,
    root_canonical: String,
) -> Result<Schema, String> {
    let backend = to_ipc(state.studio_registry.get(&format_id))?;
    to_ipc(backend.schema_load(source, root_canonical).await)
}

#[tauri::command]
pub async fn studio_schema_view_source(
    state:          State<'_, AppState>,
    format_id:      String,
    source:         String,
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
#[tauri::command]
pub async fn studio_rename_preview(
    state:          State<'_, AppState>,
    format_id:      String,
    tab_id:         String,
    old_value:      String,
    new_value_hint: Option<String>,
    open_docs:      Vec<RenameOpenDoc>,
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
#[tauri::command]
pub async fn studio_rename_apply(
    state:     State<'_, AppState>,
    format_id: String,
    tab_id:    String,
    old_value: String,
    new_value: String,
    sites:     Vec<RenameSite>,
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

#[tauri::command]
pub async fn studio_bulk_edit_preview(
    state:        State<'_, AppState>,
    format_id:    String,
    tab_id:       String,
    doc_id:       String,
    scope:        BulkEditScope,
    query:        String,
    action:       BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    open_docs:    Vec<BulkEditOpenDoc>,
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

#[tauri::command]
pub async fn studio_bulk_edit_apply(
    state:        State<'_, AppState>,
    format_id:    String,
    tab_id:       String,
    doc_id:       String,
    scope:        BulkEditScope,
    action:       BulkEditAction,
    value_source: Option<BulkEditValueSource>,
    sites:        Vec<BulkEditSite>,
    open_docs:    Vec<BulkEditOpenDoc>,
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
