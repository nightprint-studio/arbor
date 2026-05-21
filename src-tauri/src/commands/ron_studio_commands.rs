//! Tauri commands backing the RON Studio plugin and its modal viewer.
//!
//! The frontend holds the canonical edited text (it's a textarea) and pushes
//! it back via `ron_studio_set_text` whenever the user types. The host keeps
//! the parsed `ast::RonAst` cached so the Tree view stays cheap; parse errors
//! propagate to the UI without throwing.
//!
//! Save / Save As: text is written to disk through `ron_studio_save`. The
//! command accepts an explicit `contents` parameter rather than reading from
//! the registry so the frontend can save even when the host-side parsed
//! state is stale (we just wrote, no point re-parsing first).

use tauri::State;

use crate::error::{AppError, Result};
use crate::ron_studio::schema::{self, CrateProbe, Schema};
use crate::ron_studio::{
    DiffHunk, DiffTreeNode, DocSnapshot, MutateResult, NodeView,
    ParseResult, PrimitiveValue, RonFileEntry, RonQueryHit, UpdateResult,
};
use crate::AppState;

#[tauri::command]
pub async fn ron_studio_parse(
    state:         State<'_, AppState>,
    text:          Option<String>,
    path:          Option<String>,
    // Studio context for binding resolution. When the file lives
    // outside the repo (an external sidebar entry, save game, …),
    // `ron_studio`'s walk-up sidecar lookup from `path` can't reach
    // the repo's `.ron-studio.toml`. Passing the active tab + the
    // synthetic relative path lets the host fall back to a
    // cfg-keyed lookup so the schema badge survives reopening.
    tab_id:        Option<String>,
    relative_path: Option<String>,
) -> Result<ParseResult> {
    let (text, source_path) = match (text, path) {
        (Some(t), p) => (t, p),
        (None, Some(p)) => {
            let content = std::fs::read_to_string(&p)
                .map_err(|e| AppError::Other(format!("Cannot read {p}: {e}")))?;
            (content, Some(p))
        }
        (None, None) => return Err(AppError::Other("ron_studio_parse: provide `text` or `path`".into())),
    };
    // Resolve the repo path up-front so we can release the repo lock
    // before grabbing the (separate) ron-studio registry lock.
    let repo_path = match tab_id.as_deref() {
        Some(t) => state.lock_repos().ok().and_then(|mut m| m.get(t).ok().map(|r| r.path.clone())),
        None    => None,
    };
    let mut result = {
        let mut reg = state.lock_ron_studio()?;
        reg.parse(text, source_path)?
    };
    // Fall back to a cfg-keyed binding lookup when the in-text
    // detection found nothing — covers external files whose path
    // sits nowhere near the repo's `.ron-studio.toml`.
    if result.schema_hint.is_none() {
        if let (Some(repo), Some(rel)) = (repo_path, relative_path) {
            let cfg = crate::studio::config::load(&repo).unwrap_or_default();
            if let Some((rs_file, root_type)) =
                crate::studio::config::resolve_binding(&cfg, &repo, &rel)
            {
                result.schema_hint = Some(crate::ron_studio::SchemaHint {
                    rs_file,
                    root_type,
                    origin: crate::ron_studio::SchemaHintOrigin::Sidecar,
                });
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn ron_studio_close(state: State<'_, AppState>, doc_id: String) -> Result<()> {
    state.lock_ron_studio()?.close(&doc_id);
    Ok(())
}

#[tauri::command]
pub fn ron_studio_set_text(
    state:   State<'_, AppState>,
    doc_id:  String,
    text:    String,
) -> Result<UpdateResult> {
    state.lock_ron_studio()?.set_text(&doc_id, text)
}

#[tauri::command]
pub fn ron_studio_get_root(state: State<'_, AppState>, doc_id: String) -> Result<Option<NodeView>> {
    state.lock_ron_studio()?.get_root(&doc_id)
}

#[tauri::command]
pub fn ron_studio_get_children(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
) -> Result<Vec<NodeView>> {
    state.lock_ron_studio()?.get_children(&doc_id, &path)
}

#[tauri::command]
pub fn ron_studio_get_value(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
) -> Result<String> {
    state.lock_ron_studio()?.get_value_pretty(&doc_id, &path)
}

/// Full-RFC-9535 JSONPath over the parsed RON AST. See the doc comment
/// on `RonStudioRegistry::query` for the projection rules and shorthand
/// normalisations. Sync command — query work is cheap relative to the
/// IPC round-trip the frontend already performs, and serde_json_path
/// is small enough that we don't need a worker thread.
#[tauri::command]
pub fn ron_studio_query(
    state:  State<'_, AppState>,
    doc_id: String,
    expr:   String,
) -> Result<Vec<RonQueryHit>> {
    state.lock_ron_studio()?.query(&doc_id, &expr)
}

#[tauri::command]
pub fn ron_studio_raw_original(state: State<'_, AppState>, doc_id: String) -> Result<String> {
    state.lock_ron_studio()?.raw_original(&doc_id)
}

#[tauri::command]
pub fn ron_studio_raw_current(state: State<'_, AppState>, doc_id: String) -> Result<String> {
    state.lock_ron_studio()?.raw_current(&doc_id)
}

#[tauri::command]
pub fn ron_studio_format(state: State<'_, AppState>, doc_id: String) -> Result<String> {
    state.lock_ron_studio()?.format(&doc_id)
}

#[tauri::command]
pub fn ron_studio_to_json(state: State<'_, AppState>, doc_id: String) -> Result<String> {
    state.lock_ron_studio()?.to_json(&doc_id)
}

#[tauri::command]
pub fn ron_studio_from_json(state: State<'_, AppState>, doc_id: String, json_text: String) -> Result<String> {
    state.lock_ron_studio()?.from_json(&doc_id, &json_text)
}

#[tauri::command]
pub fn ron_studio_get_indent(state: State<'_, AppState>, doc_id: String) -> Result<String> {
    state.lock_ron_studio()?.get_indent(&doc_id)
}

#[tauri::command]
pub fn ron_studio_set_indent(
    state:  State<'_, AppState>,
    doc_id: String,
    indent: String,
) -> Result<()> {
    state.lock_ron_studio()?.set_indent(&doc_id, indent)
}

/// View the Rust source of a schema-resolved named type. Runs schema
/// walking on a background thread because `syn::parse_file` is CPU work
/// across potentially hundreds of source files.
#[tauri::command]
pub async fn ron_studio_schema_view_source(
    rs_file:        String,
    canonical_path: String,
) -> Result<schema::TypeSource> {
    tokio::task::spawn_blocking(move || schema::get_type_source(&rs_file, &canonical_path))
        .await
        .map_err(|e| AppError::Other(format!("schema view-source join: {e}")))?
}

/// Write the supplied contents to disk. If `bind_to_doc` is true the
/// document's `source_path` is updated to the new path (for Save As). The
/// document's "original" snapshot is also refreshed so the diff view goes
/// empty after save.
#[tauri::command]
pub fn ron_studio_save(
    state:        State<'_, AppState>,
    doc_id:       String,
    path:         String,
    contents:     String,
    bind_to_doc:  bool,
) -> Result<()> {
    crate::ron_studio::write_to_disk(&path, &contents)?;
    let mut reg = state.lock_ron_studio()?;
    if bind_to_doc {
        reg.rebind_source(&doc_id, path)?;
    }
    reg.mark_saved(&doc_id)?;
    Ok(())
}

#[tauri::command]
pub fn ron_studio_source_path(state: State<'_, AppState>, doc_id: String) -> Result<Option<String>> {
    state.lock_ron_studio()?.source_path(&doc_id)
}

#[tauri::command]
pub fn ron_studio_diff(state: State<'_, AppState>, doc_id: String) -> Result<Vec<DiffHunk>> {
    state.lock_ron_studio()?.diff(&doc_id)
}

#[tauri::command]
pub fn ron_studio_tree_diff(state: State<'_, AppState>, doc_id: String) -> Result<DiffTreeNode> {
    state.lock_ron_studio()?.tree_diff(&doc_id)
}

#[tauri::command]
pub fn ron_studio_undo(state: State<'_, AppState>, doc_id: String) -> Result<MutateResult> {
    state.lock_ron_studio()?.undo(&doc_id)
}

#[tauri::command]
pub fn ron_studio_redo(state: State<'_, AppState>, doc_id: String) -> Result<MutateResult> {
    state.lock_ron_studio()?.redo(&doc_id)
}

#[tauri::command]
pub fn ron_studio_history_state(state: State<'_, AppState>, doc_id: String) -> Result<(bool, bool)> {
    state.lock_ron_studio()?.history_state(&doc_id)
}

#[tauri::command]
pub fn ron_studio_snapshot(state: State<'_, AppState>, doc_id: String) -> Result<DocSnapshot> {
    state.lock_ron_studio()?.snapshot(&doc_id)
}

#[tauri::command]
pub async fn ron_studio_list_ron_files(folder: String) -> Result<Vec<RonFileEntry>> {
    tokio::task::spawn_blocking(move || crate::ron_studio::list_ron_files(&folder))
        .await
        .map_err(|e| AppError::Other(format!("list ron files join: {e}")))?
}

// ── Tree-edit mutations ────────────────────────────────────────────────────
//
// These operate on the parsed AST and regenerate the document text by
// pretty-printing. Comments and original formatting are lost — the UI
// surfaces a one-shot banner before the first tree edit so the user
// knows. Span-tracking edits are tracked as a separate v2 effort.

#[tauri::command]
pub fn ron_studio_mutate_primitive(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
    value:  PrimitiveValue,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.mutate_primitive(&doc_id, &path, value)
}

#[tauri::command]
pub fn ron_studio_toggle_option(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.toggle_option(&doc_id, &path)
}

#[tauri::command]
pub fn ron_studio_replace_at(
    state:    State<'_, AppState>,
    doc_id:   String,
    path:     Vec<String>,
    ron_text: String,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.replace_at(&doc_id, &path, ron_text)
}

#[tauri::command]
pub fn ron_studio_remove_at(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.remove_at(&doc_id, &path)
}

#[tauri::command]
pub fn ron_studio_insert_field(
    state:    State<'_, AppState>,
    doc_id:   String,
    path:     Vec<String>,
    name:     String,
    ron_text: String,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.insert_field(&doc_id, &path, name, ron_text)
}

#[tauri::command]
pub fn ron_studio_insert_item(
    state:    State<'_, AppState>,
    doc_id:   String,
    path:     Vec<String>,
    ron_text: String,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.insert_item(&doc_id, &path, ron_text)
}

#[tauri::command]
pub fn ron_studio_insert_map_entry(
    state:    State<'_, AppState>,
    doc_id:   String,
    path:     Vec<String>,
    key_text: String,
    val_text: String,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.insert_map_entry(&doc_id, &path, key_text, val_text)
}

#[tauri::command]
pub fn ron_studio_duplicate_at(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.duplicate_at(&doc_id, &path)
}

#[tauri::command]
pub fn ron_studio_move_item(
    state:  State<'_, AppState>,
    doc_id: String,
    path:   Vec<String>,
    delta:  i32,
) -> Result<MutateResult> {
    state.lock_ron_studio()?.move_item(&doc_id, &path, delta)
}

// ── Schema loading ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ron_studio_schema_probe(rs_file: String) -> Result<CrateProbe> {
    tokio::task::spawn_blocking(move || schema::probe(&rs_file))
        .await
        .map_err(|e| AppError::Other(format!("schema probe join: {e}")))?
}

#[tauri::command]
pub async fn ron_studio_schema_load(
    rs_file:        String,
    root_canonical: String,
) -> Result<Schema> {
    tokio::task::spawn_blocking(move || schema::load(&rs_file, &root_canonical))
        .await
        .map_err(|e| AppError::Other(format!("schema load join: {e}")))?
}
