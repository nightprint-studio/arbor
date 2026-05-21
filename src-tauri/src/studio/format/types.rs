//! Shared, format-agnostic data shapes exchanged between the studio
//! commands layer and per-format backends.
//!
//! Discriminating kinds (e.g. RON's `named_struct` / `unit_variant`,
//! TOML's `inline_table` / `array_of_tables`) are passed as **opaque
//! `String`** here — the per-format `FormatDescriptor.kind_palette`
//! tells the UI how to style each one. We deliberately don't collapse
//! kinds into a "lowest common denominator" enum (FROZEN F11).
//!
//! Shapes mirror what `ron_studio` already serialises to the
//! frontend, so the migration to the unified trait is wire-compatible.

use serde::{Deserialize, Serialize};

// Re-exports — Rust-source schema introspection lives in ron_studio
// today and is reused as-is by any format that binds to .rs structs.
pub use crate::ron_studio::schema::{CrateProbe, Schema, TypeSource};

/// Outcome of opening or re-opening a document.
#[derive(Debug, Serialize)]
pub struct ParseResult {
    pub doc_id:      String,
    pub size_bytes:  usize,
    pub source_path: Option<String>,
    pub original:    String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<String>,
    pub child_count: usize,
    pub schema_hint: Option<SchemaHint>,
    /// Encoding sniffed from the file bytes at open time (FROZEN F16).
    /// Remembered per-doc so save can re-encode losslessly. UTF-8 / no
    /// BOM when the FE pushed text directly without a backing file.
    pub encoding:    EncodingInfo,

    /// `true` when the backend decided to open this doc in "stream"
    /// mode (size ≥ format's streaming threshold). Stream-mode docs
    /// trade structural editing (mutations disabled) for cheap parse +
    /// navigation on large files. JSON Phase 3.d is the first consumer
    /// (`simd_json` strict only above 1 MB). Other formats default
    /// `false` until they wire their own streaming path.
    #[serde(default)]
    pub stream_mode: bool,

    /// Phase 3.d (JSON Studio only — other backends leave as `false`).
    /// `true` when the doc was opened from a `.jsonc` file. Drives the
    /// banner FE: `.jsonc + features` is expected, `.json + features`
    /// surfaces the rename/strip prompt.
    #[serde(default)]
    pub is_jsonc: bool,

    /// Phase 3.d (JSON Studio only). `true` when the current buffer
    /// contains comments or trailing commas — strict JSON parsers
    /// would reject it.
    #[serde(default)]
    pub has_jsonc_features: bool,
}

/// Encoding metadata for a doc — label + whether the original buffer
/// shipped with a BOM. Round-tripped through save so legacy files
/// (windows-1252 .properties, UTF-16 BOM .yaml) survive an edit cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingInfo {
    /// Canonical encoding name as reported by `encoding_rs` (e.g.
    /// `"UTF-8"`, `"windows-1252"`, `"UTF-16LE"`).
    pub label:   String,
    pub had_bom: bool,
}

impl EncodingInfo {
    /// Default for FE-pushed text without a backing file.
    pub fn utf8() -> Self {
        Self { label: "UTF-8".into(), had_bom: false }
    }
}

impl Default for EncodingInfo {
    fn default() -> Self { Self::utf8() }
}

/// Outcome of a text-level edit (textarea typing).
#[derive(Debug, Serialize)]
pub struct UpdateResult {
    pub parse_error: Option<String>,
    pub root_kind:   Option<String>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
    /// Phase 3.d (JSON only). Recomputed on every text edit so the FE
    /// banner stays in sync with the live buffer.
    #[serde(default)]
    pub has_jsonc_features: bool,
}

/// Outcome of a structured tree-edit (`apply_mutation` / `undo` / `redo`).
/// Carries the regenerated text so the FE can refresh in a single round-trip.
#[derive(Debug, Serialize)]
pub struct MutateResult {
    pub text:        String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<String>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
    /// Phase 3.d (JSON only). Recomputed after the mutation.
    #[serde(default)]
    pub has_jsonc_features: bool,
}

/// One row in the tree pane (root, child of a container, etc.).
#[derive(Debug, Serialize, Clone)]
pub struct NodeView {
    pub key:         String,
    pub path:        Vec<String>,
    pub kind:        String,
    pub preview:     String,
    pub child_count: usize,
    pub variant_tag: Option<String>,
}

/// One hit in a JSONPath query.
#[derive(Debug, Serialize, Clone)]
pub struct QueryHit {
    pub path:        Vec<String>,
    pub kind:        String,
    pub preview:     String,
    pub variant_tag: Option<String>,
}

/// Tagged enum that drives all structured tree-edits. Per-format
/// backends destructure this and delegate to their native mutation
/// helpers. `value` for `SetPrimitive` is an opaque `serde_json::Value`
/// — the backend decides how to coerce it for its own AST (RON has its
/// own `PrimitiveValue` enum it deserialises into).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StudioMutation {
    SetPrimitive   { path: Vec<String>, value: serde_json::Value },
    ToggleOption   { path: Vec<String> },
    ReplaceAt      { path: Vec<String>, text: String },
    RemoveAt       { path: Vec<String> },
    InsertField    { path: Vec<String>, name: String, text: String },
    InsertItem     { path: Vec<String>, text: String },
    InsertMapEntry { path: Vec<String>, key_text: String, val_text: String },
    DuplicateAt    { path: Vec<String> },
    MoveItem       { path: Vec<String>, delta: i32 },
}

/// One row inside a text-level diff hunk.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiffLine {
    pub kind:     DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text:     String,
}

/// Text-level diff hunk between `original` and `current`.
#[derive(Debug, Serialize, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines:     Vec<DiffLine>,
}

/// Tree-level diff status applied to a node in `DiffTreeNode`.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    /// Both sides equal — pruned from the parent before being sent.
    Unchanged,
    /// New leaf or subtree in `current` that didn't exist in `original`.
    Added,
    /// Leaf or subtree in `original` that's gone in `current`.
    Removed,
    /// Different leaf value at the same path.
    Modified,
    /// Container "unchanged in shape" but at least one descendant differs.
    Partial,
}

/// Recursive tree-diff node.
#[derive(Debug, Serialize, Clone)]
pub struct DiffTreeNode {
    pub key:             String,
    pub path:            Vec<String>,
    pub status:          DiffStatus,
    pub kind_before:     Option<String>,
    pub kind_after:      Option<String>,
    pub preview_before:  Option<String>,
    pub preview_after:   Option<String>,
    pub tag_before:      Option<String>,
    pub tag_after:       Option<String>,
    pub children:        Vec<DiffTreeNode>,
    pub change_count:    u32,
}

/// In-memory snapshot of a doc — used by the workspace store to
/// rehydrate tabs and to populate the diff view.
#[derive(Debug, Serialize, Clone)]
pub struct DocSnapshot {
    pub doc_id:      String,
    pub source_path: Option<String>,
    pub size_bytes:  usize,
    pub original:    String,
    pub current:     String,
    pub parse_error: Option<String>,
    pub root_kind:   Option<String>,
    pub child_count: usize,
    pub can_undo:    bool,
    pub can_redo:    bool,
    pub indent:      String,
}

/// One entry in a "list files of this format under <folder>" response.
#[derive(Debug, Serialize, Clone)]
pub struct FileEntry {
    pub absolute_path: String,
    pub relative_path: String,
    pub name:          String,
    pub size_bytes:    u64,
}

/// Hint that links a doc to its schema source (a `.rs` file today,
/// possibly `.java` or JSON Schema later — see Phase 7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaHint {
    pub rs_file:   String,
    pub root_type: String,
    pub origin:    SchemaHintOrigin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaHintOrigin {
    /// Inline directive at the top of the doc, e.g. `//! ron-studio: schema=…`.
    Directive,
    /// `.studio.toml` (or legacy `.ron-studio.toml`) side-car match.
    Sidecar,
}

// ── F12 — Cross-reference rename refactor ────────────────────────────────────
//
// Format-agnostic shapes for the project-wide rename refactor (FROZEN
// F12). Each format implementing `rename_preview` / `rename_apply`
// returns these. The `scope` enum carries the per-format semantics
// (definition vs reference, plus `Key` for `.properties`).

/// One occurrence of `old_value` somewhere in the project — either a
/// definition (`id:`/`name:` field) or a reference field whose value
/// matches.
///
/// `Deserialize` is needed because the FE echoes the (possibly
/// user-pruned) site list back into `studio_rename_apply`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameSite {
    /// Absolute path of the file holding the site.
    pub absolute_path: String,
    /// Repo-relative POSIX path — same shape `studio::scan_repo` uses.
    pub relative_path: String,
    /// Plain file name for label rendering.
    pub file_name:     String,
    /// AST path of the value node (matches what `studio_get_value`
    /// expects).
    pub field_path:    Vec<String>,
    /// Key the value lives under (`id`, `name`, `enemy_id`, …). For
    /// `.properties` `Key` scope this is the dotted key itself.
    pub key_name:      String,
    pub scope:         RenameSiteScope,
    /// Short snippet of the line for the preview UI ("(id: \"goblin\", …)").
    /// Best-effort; empty when the backend can't easily synthesise one.
    #[serde(default)]
    pub preview:       String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenameSiteScope {
    /// `id:`/`name:` field (def). Renaming this re-targets every
    /// downstream reference.
    Definition,
    /// Reference field whose string value equals `old_value`.
    Reference,
    /// `.properties` only — the dotted key itself (LHS of `=`).
    Key,
}

/// Affected open doc that's currently dirty — blocks the apply step
/// per FROZEN F12 ("Some affected files have unsaved changes. Save or
/// discard first.").
#[derive(Debug, Clone, Serialize)]
pub struct RenameDirtyBlocker {
    pub doc_id:      String,
    pub source_path: Option<String>,
}

/// One existing site whose value already equals the target `new_value`
/// — surfaced as a "Target already exists" warning the user can
/// override (FROZEN F12: sticky warning, not a hard block).
#[derive(Debug, Clone, Serialize)]
pub struct RenameCollision {
    pub absolute_path: String,
    pub relative_path: String,
    pub field_path:    Vec<String>,
    pub key_name:      String,
}

/// Information about a doc currently open in a tab — passed by the FE
/// so the BE can compute dirty blockers without knowing the FE state.
#[derive(Debug, Clone, Deserialize)]
pub struct RenameOpenDoc {
    pub doc_id:      String,
    pub source_path: Option<String>,
    pub dirty:       bool,
}

/// Output of `rename_preview`. The FE renders the site list grouped
/// by file, applies user-side checkbox skips, and feeds the surviving
/// sites into `rename_apply`.
#[derive(Debug, Serialize)]
pub struct RenamePreview {
    pub sites:           Vec<RenameSite>,
    /// Files whose currently-open in-app docs have unsaved changes.
    /// Non-empty list ⇒ apply is blocked by FROZEN F12.
    pub dirty_blockers:  Vec<RenameDirtyBlocker>,
    /// Existing definitions/references whose value already equals the
    /// (preview-time) new value — empty when no `new_value` was hinted
    /// at preview time.
    pub collisions:      Vec<RenameCollision>,
}

/// Failure record from a partial flush mid-apply (FROZEN F12: NO
/// rollback of files already written; user sees a warning toast).
#[derive(Debug, Serialize)]
pub struct RenameFailure {
    pub absolute_path: String,
    pub message:       String,
}

/// Output of `rename_apply`. `written_files` is what landed on disk
/// successfully; `failed_files` is the set the BE could not flush
/// (validation already passed in the in-memory pre-flush stage —
/// these are IO errors only).
#[derive(Debug, Serialize)]
pub struct RenameResult {
    pub written_files: Vec<String>,
    pub failed_files:  Vec<RenameFailure>,
}

// ── F13 — Query-driven bulk edit (mini-expression language) ──────────
//
// `set` and `delete` over the hits of a JSONPath query. The value
// source is either a typed literal or a mini-expression compiled by
// `studio::edit_expr`. RON is the prototype consumer (Phase 2B-4);
// JSON/TOML/YAML/.properties inherit the same shapes once their
// backends ship `bulk_edit_preview` / `bulk_edit_apply`.

/// What the bulk edit does at each query hit.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BulkEditAction {
    /// Replace the value at each hit with `value_source`.
    Set,
    /// Remove the node from its parent (list/tuple/struct/map). For
    /// format-specific edge cases see the format's `null_handling` and
    /// the prose in FROZEN F13 (RON's option-delete semantics, etc.).
    Delete,
}

/// Typed literal used when `value_source = literal`. The variant
/// chooses the wire kind; per-format coercion at apply time decides
/// whether the literal fits the node's current kind (e.g. setting a
/// `string` literal on a RON `int` site → that site is skipped with
/// a "type mismatch" warning in the preview).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum BulkEditLiteral {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

/// Where the new value comes from for `BulkEditAction::Set`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BulkEditValueSource {
    /// Static literal — same value installed at every hit.
    Literal     { literal: BulkEditLiteral },
    /// Mini-expression compiled by `studio::edit_expr`. Evaluated once
    /// per site with `old` bound to the current value at that hit.
    /// See `studio/edit_expr.rs` for the grammar + built-ins.
    Expression  { source: String },
}

/// Scope of the bulk edit. `ActiveDoc` mutates only the open doc's
/// in-memory text (a single `set_text` history entry); `ProjectWide`
/// reads + writes every affected file with F12's atomic flush + F16
/// per-file encoding preservation.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BulkEditScope {
    ActiveDoc,
    ProjectWide,
}

/// One hit-site in the preview list. `field_path` is the same shape
/// used by `studio_get_value` etc. `new_preview` carries the computed
/// new value for the user-visible "old → new" line in the modal —
/// `None` only when the site will be skipped (preview-time validation
/// failure, expression eval error, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkEditSite {
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    pub field_path:    Vec<String>,
    /// Opaque per-format kind string — same shape as `NodeView.kind`.
    pub kind:          String,
    /// Short preview of the current value at this site (`"goblin"`,
    /// `42`, `true`, …) — used for the "old →" half of the diff line.
    pub old_preview:   String,
    /// Short preview of the computed new value, when available. Empty
    /// when `will_skip = true`.
    pub new_preview:   String,
    /// Whether this site will be skipped at apply time (eval error,
    /// container hit on `set`, RON-null on non-option, …). The
    /// preview list still includes it so the user sees what was
    /// dropped + why.
    pub will_skip:     bool,
    /// Human-readable skip reason when `will_skip = true`. Empty
    /// otherwise. The modal shows this as a per-site warning chip.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub skip_reason:   String,
}

/// Open-doc snapshot for the dirty-blocker check — same shape as F12.
pub type BulkEditOpenDoc = RenameOpenDoc;

/// Output of `bulk_edit_preview`. `expression_error` is set only when
/// the compile pass of `value_source = expression` failed — surface as
/// a top-level banner, NOT per-site. Per-site eval errors land in the
/// site list as `will_skip = true` + `skip_reason`.
#[derive(Debug, Serialize)]
pub struct BulkEditPreview {
    pub sites:            Vec<BulkEditSite>,
    pub dirty_blockers:   Vec<RenameDirtyBlocker>,
    pub expression_error: Option<String>,
}

/// Per-file IO failure mid-flush (rare — see FROZEN F12 partial-flush
/// policy). Same shape as `RenameFailure`; we keep a distinct type so
/// the FE wire stays explicit.
#[derive(Debug, Serialize)]
pub struct BulkEditFailure {
    pub absolute_path: String,
    pub message:       String,
}

/// Output of `bulk_edit_apply`. `applied_sites` + `skipped_sites`
/// count every site the FE sent in; `written_files` / `failed_files`
/// only apply to `ProjectWide` scope. For `ActiveDoc` scope the active
/// doc's in-memory text is mutated and `written_files` is empty;
/// `active_doc_state` carries the post-mutation snapshot (same shape
/// as `apply_mutation` returns) so the FE syncs without a second
/// round-trip.
#[derive(Debug, Serialize)]
pub struct BulkEditResult {
    pub written_files:    Vec<String>,
    pub failed_files:     Vec<BulkEditFailure>,
    pub applied_sites:    usize,
    pub skipped_sites:    usize,
    /// MutateResult for the active doc when scope = ActiveDoc. `None`
    /// for project-wide. Same shape as `studio_apply_mutation` so the
    /// FE pipes it straight through `applyMutateResult`.
    pub active_doc_state: Option<MutateResult>,
}
