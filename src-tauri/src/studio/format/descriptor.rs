//! `FormatDescriptor` — what a format tells the UI about itself.
//!
//! The descriptor is the **source of truth** for format-specific UI
//! behaviour: capability flags, kind palette, null-handling policy,
//! save-warning hints, etc. The generic `StudioModal.svelte` consults
//! this instead of branching on `format_id` (FROZEN F17).
//!
//! Capabilities that show up here pair with method gates on the
//! `StudioFormatBackend` trait — calling an opt-in method on a format
//! that doesn't support it returns `StudioError::Unsupported`. The FE
//! is expected to check the descriptor first and never blindly probe.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct FormatDescriptor {
    pub id:                          String,
    pub label:                       String,
    pub file_extensions:             Vec<String>,
    pub icon:                        IconRef,

    // ── Edit semantics ───────────────────────────────────────────────
    pub supports_lossless_edit:      bool,
    pub supports_comments:           bool,
    pub supports_anchors:            bool,
    pub null_handling:               NullPolicy,

    // ── Tree/Render hints (F15) ──────────────────────────────────────
    pub supports_streaming_mode:     bool,
    pub streaming_threshold_kb:      Option<u32>,
    pub streaming_setting_key:       Option<String>,

    // ── Query (F6) ───────────────────────────────────────────────────
    pub query_syntax:                QuerySyntax,

    // ── Cross-refs (F5) ──────────────────────────────────────────────
    pub cross_ref_default_fields:    Vec<String>,
    pub cross_ref_scopes:            Vec<CrossRefScope>,

    // ── Schema (F4 + Phase 7) ────────────────────────────────────────
    pub schema_sources:              Vec<SchemaSourceKind>,

    // ── Kind palette — replaces hard-coded per-format chips (F11) ────
    pub kind_palette:                BTreeMap<String, KindStyle>,

    // ── Save behavior + banners (F9 lossy / F14 jsonc) ───────────────
    pub save_warnings:               Vec<SaveWarningKind>,
    pub save_behavior_setting_key:   Option<String>,

    // ── Convert ──────────────────────────────────────────────────────
    pub convert_to_json_supported:   bool,

    // ── External files (cfg-keyed schema resolution) ─────────────────
    pub supports_external_files:     bool,

    // ── F12 — Cross-reference rename refactor ────────────────────────
    //
    // `true` when the backend implements `rename_preview` /
    // `rename_apply`. The FE gates the "Rename across project…"
    // context-menu item on this flag — never probes by attempting.
    pub supports_rename_reference:   bool,

    // ── F13 — Query-driven bulk edit + mini-expression language ──────
    //
    // `true` when the backend implements `bulk_edit_preview` /
    // `bulk_edit_apply`. The FE gates the `[⚡ Edit]` button in the
    // `StudioQueryBar` on this flag — never probes by attempting.
    pub supports_bulk_edit:          bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IconRef {
    /// Iconify identifier — embedded at build-time via the configured
    /// Iconify Vite plugin (no network fetch at runtime — FROZEN F8).
    Iconify { name: String },
    /// Inline SVG markup, used when no Iconify glyph fits.
    InlineSvg { svg: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NullPolicy {
    /// Format has a real null (JSON, YAML).
    Native,
    /// Format has no null — setting null deletes the key (TOML).
    AsDelete,
    /// Format has no null — user picks per-edit (empty value vs delete) (.properties).
    AskUser,
    /// Format has no null at all (RON: use Option / None).
    NotSupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuerySyntax {
    /// RFC 9535 JSONPath — the unified query syntax in MVP.
    JsonPath,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossRefScope {
    /// Refactor a value reference (the right-hand side of an assignment).
    Value,
    /// Refactor a key declaration (left-hand side) — `.properties` only.
    Key,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaSourceKind {
    /// `.rs` file walked with `syn` (today's RON/TOML schema source).
    RustStruct,
    /// `.schema.json` JSON Schema file.
    JsonSchema,
    /// `.java` file walked with tree-sitter — Phase 7 only.
    JavaClass,
}

/// UI styling hint per kind. Lets each format paint its own chip
/// colours without the shell hard-coding format-specific kind lists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindStyle {
    pub label: String,
    pub tone:  KindTone,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon:  Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KindTone {
    Neutral,
    Info,
    Accent,
    Success,
    Warning,
    Error,
    Muted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveWarningKind {
    /// "Saving will lose comments and reformat" (F9 YAML).
    LossyComments,
    /// "JSON file uses JSONC features (comments / trailing commas)" (F14).
    JsoncCommentsInJson,
}
