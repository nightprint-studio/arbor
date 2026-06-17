//! `format` domain — the format-agnostic Studio editor surface (RON / JSON /
//! TOML / YAML / .properties) dispatched through `AppState.studio_registry`.
//!
//! Each handler is the body the matching `#[tauri::command] fn` ran, now a plain
//! sync function self-registered under `program = "studio"`. Every call takes
//! `format_id` as its first JSON argument and resolves the backend via the
//! registry; `StudioError` is mapped to a wire string through `errors::to_ipc`.
//!
//! Only the **synchronous** slice of the old `studio::format::commands` lives
//! here. The genuinely-async commands (`studio_parse`, `studio_save`,
//! `studio_list_files`, the schema probes, and the F12/F13 rename / bulk-edit
//! flows) `.await` a real backend future and stay inline as keep-shell Tauri
//! commands — the `#[studio::handler]` macro is sync-only.

use crate::error::AppError;
use crate::ipc::studio;
use crate::studio::format::descriptor::FormatDescriptor;
use crate::studio::format::errors::to_ipc;
use crate::studio::format::properties_codec::{
    self, PropertiesToYamlOptions, PropertiesToYamlOutput, YamlToPropertiesOutput,
};
use crate::studio::format::types::{
    DiffHunk, DiffTreeNode, DocSnapshot, EncodingInfo, MutateResult, NodeView, QueryHit,
    StudioMutation, UpdateResult,
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
