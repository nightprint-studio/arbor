//! `StudioFormatBackend` — the trait each format (RON, JSON, TOML,
//! YAML, .properties) implements.
//!
//! All methods take `&self` so backends can be stored in
//! `Arc<dyn StudioFormatBackend>` inside the registry. Interior
//! mutability (typically a `Mutex` wrapping the per-format doc state)
//! is the backend's concern, not the trait's.
//!
//! Optional capabilities have default impls that return
//! `StudioError::Unsupported`. The FE consults
//! `FormatDescriptor.*` flags to decide whether to call them — never
//! probes by attempting and catching.

use async_trait::async_trait;

use super::descriptor::FormatDescriptor;
use super::errors::{StudioError, StudioResult};
use super::types::{
    BulkEditAction, BulkEditOpenDoc, BulkEditPreview, BulkEditResult,
    BulkEditScope, BulkEditSite, BulkEditValueSource, CrateProbe, DiffHunk,
    DiffTreeNode, DocSnapshot, EncodingInfo, FileEntry, MutateResult, NodeView,
    ParseResult, QueryHit, RenameOpenDoc, RenamePreview, RenameResult,
    RenameSite, Schema, StudioMutation, TypeSource, UpdateResult,
};

#[async_trait]
pub trait StudioFormatBackend: Send + Sync {
    // ── Descriptor ───────────────────────────────────────────────────
    fn descriptor(&self) -> &FormatDescriptor;

    // ── Lifecycle ────────────────────────────────────────────────────
    //
    // `text` is the in-memory content (the FE pushed it, or the
    // command layer read the file). `source_path` is the on-disk
    // origin, used by the FE to display the file name and by save.
    // Sidecar / cfg-keyed schema_hint resolution is the **command**'s
    // job — the backend just returns whatever hint it can find inline.
    async fn parse(
        &self,
        text:        String,
        source_path: Option<String>,
        encoding:    EncodingInfo,
    ) -> StudioResult<ParseResult>;
    fn close(&self, doc_id: &str) -> StudioResult<()>;

    // ── Encoding (FROZEN F16) ────────────────────────────────────────
    //
    // Backends remember the sniffed encoding of each open doc so save
    // can round-trip windows-1252 / UTF-16 BOM files losslessly.
    fn get_encoding(&self, doc_id: &str) -> StudioResult<EncodingInfo>;

    // ── Text & raw access ────────────────────────────────────────────
    fn set_text(&self, doc_id: &str, text: String) -> StudioResult<UpdateResult>;
    fn raw_original(&self, doc_id: &str) -> StudioResult<String>;
    fn raw_current(&self, doc_id: &str) -> StudioResult<String>;
    fn format_doc(&self, doc_id: &str) -> StudioResult<String>;
    fn get_indent(&self, doc_id: &str) -> StudioResult<String>;
    fn set_indent(&self, doc_id: &str, indent: String) -> StudioResult<()>;

    // ── Tree navigation ──────────────────────────────────────────────
    fn get_root(&self, doc_id: &str) -> StudioResult<Option<NodeView>>;
    fn get_children(&self, doc_id: &str, path: Vec<String>) -> StudioResult<Vec<NodeView>>;
    fn get_value(&self, doc_id: &str, path: Vec<String>) -> StudioResult<String>;

    // ── Query ────────────────────────────────────────────────────────
    fn query(&self, doc_id: &str, expr: String) -> StudioResult<Vec<QueryHit>>;

    // ── Mutations ────────────────────────────────────────────────────
    fn apply_mutation(
        &self,
        doc_id: &str,
        mutation: StudioMutation,
    ) -> StudioResult<MutateResult>;

    // ── Diff ─────────────────────────────────────────────────────────
    fn diff(&self, doc_id: &str) -> StudioResult<Vec<DiffHunk>>;
    fn tree_diff(&self, doc_id: &str) -> StudioResult<DiffTreeNode>;

    // ── History ──────────────────────────────────────────────────────
    fn undo(&self, doc_id: &str) -> StudioResult<MutateResult>;
    fn redo(&self, doc_id: &str) -> StudioResult<MutateResult>;
    fn history_state(&self, doc_id: &str) -> StudioResult<(bool, bool)>;

    // ── Snapshot & persistence ───────────────────────────────────────
    fn snapshot(&self, doc_id: &str) -> StudioResult<DocSnapshot>;
    fn source_path(&self, doc_id: &str) -> StudioResult<Option<String>>;
    async fn save(
        &self,
        doc_id: &str,
        path: String,
        contents: String,
        bind_to_doc: bool,
    ) -> StudioResult<()>;

    // ── File listing ─────────────────────────────────────────────────
    async fn list_files(&self, folder: String) -> StudioResult<Vec<FileEntry>>;

    // ── Phase 3.d — Strip format-specific extras (optional) ──────────
    //
    // Lossy re-emit that removes constructs the strict baseline of the
    // format wouldn't accept. JSON Studio uses this for the "Strip
    // comments + trailing commas" save flow; future YAML could expose
    // anchor flattening through the same call. Default impl returns
    // `Unsupported` so backends without this need declare nothing.
    fn strip_features(&self, doc_id: &str) -> StudioResult<MutateResult> {
        let _ = doc_id;
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "strip_features",
        ))
    }

    // ── Convert (optional) ───────────────────────────────────────────
    fn to_json(&self, doc_id: &str) -> StudioResult<String> {
        let _ = doc_id;
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "to_json",
        ))
    }
    fn from_json(&self, doc_id: &str, json_text: String) -> StudioResult<String> {
        let _ = (doc_id, json_text);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "from_json",
        ))
    }

    // ── Schema (optional) ────────────────────────────────────────────
    async fn schema_probe(&self, source: String) -> StudioResult<CrateProbe> {
        let _ = source;
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "schema_probe",
        ))
    }
    async fn schema_load(
        &self,
        source: String,
        root_canonical: String,
    ) -> StudioResult<Schema> {
        let _ = (source, root_canonical);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "schema_load",
        ))
    }
    async fn schema_view_source(
        &self,
        source: String,
        canonical_path: String,
    ) -> StudioResult<TypeSource> {
        let _ = (source, canonical_path);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "schema_view_source",
        ))
    }

    // ── F12 — Cross-reference rename refactor (optional) ─────────────
    //
    // Backends declare support via `descriptor.supports_rename_reference
    // = true`. The FE gates the menu item on the flag and never probes
    // by attempting the call.
    //
    // `rename_preview` collects every site (defs + refs) where the value
    // matches `old_value`, plus existing-target collisions when a
    // `new_value_hint` is provided, plus dirty-doc blockers for any
    // affected file currently dirty in the FE state (`open_docs`).
    async fn rename_preview(
        &self,
        repo_root:      String,
        old_value:      String,
        new_value_hint: Option<String>,
        open_docs:      Vec<RenameOpenDoc>,
    ) -> StudioResult<RenamePreview> {
        let _ = (repo_root, old_value, new_value_hint, open_docs);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "rename_preview",
        ))
    }

    /// Apply the rename. Best-effort sequential with rollback PRE-flush:
    /// every affected file is parsed and rewritten in memory first; if
    /// any in-memory step fails the whole batch aborts before any disk
    /// write. If a flush mid-batch fails (rare — disk full, permission)
    /// the failure is recorded in `RenameResult.failed_files` but
    /// already-written files stay on disk (FROZEN F12: no automatic
    /// rollback of the partial — surfacing the diff is safer).
    ///
    /// Per FROZEN F16 every file is re-encoded with its own original
    /// encoding label — never globally.
    async fn rename_apply(
        &self,
        repo_root: String,
        old_value: String,
        new_value: String,
        sites:     Vec<RenameSite>,
        open_docs: Vec<RenameOpenDoc>,
    ) -> StudioResult<RenameResult> {
        let _ = (repo_root, old_value, new_value, sites, open_docs);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "rename_apply",
        ))
    }

    // ── F13 — Query-driven bulk edit (optional) ──────────────────────
    //
    // Backends declare support via `descriptor.supports_bulk_edit =
    // true`. The FE gates the `[⚡ Edit]` button + bulk-edit modal on
    // the flag and never probes by attempting.
    //
    // `bulk_edit_preview` runs the query (active doc OR project-wide),
    // computes the would-be new value for every hit, and surfaces
    // skip reasons (eval error, container hits, RON-null on non-
    // option, …) per-site. Returns dirty-doc blockers for the
    // project-wide flow (same shape as F12).
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
        let _ = (repo_root, doc_id, scope, query, action, value_source, open_docs);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "bulk_edit_preview",
        ))
    }

    /// Apply the bulk edit. Best-effort sequential with rollback
    /// PRE-flush (same policy as F12). For `ActiveDoc` scope the
    /// active doc's text is mutated through the backend's normal
    /// `set_text` history path and `active_doc_text` carries the new
    /// buffer for the FE; for `ProjectWide` every file is re-encoded
    /// with its own original encoding label (FROZEN F16, never
    /// globally) and `written_files`/`failed_files` reflect the disk
    /// flush. Per FROZEN F13 every site the FE marks as skipped is
    /// counted in `skipped_sites` but otherwise ignored.
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
        let _ = (repo_root, doc_id, scope, action, value_source, sites, open_docs);
        Err(StudioError::unsupported(
            self.descriptor_id(),
            "bulk_edit_apply",
        ))
    }
}

/// Helper for the default capability impls so they can borrow the
/// format id as `&'static str` for the `Unsupported` error variant.
/// Backends override this with a constant string ("ron" etc.).
trait StudioFormatBackendIdHelper {
    fn descriptor_id(&self) -> &'static str;
}

impl<T: StudioFormatBackend + ?Sized> StudioFormatBackendIdHelper for T {
    fn descriptor_id(&self) -> &'static str {
        // The descriptor id is a String at runtime but every backend
        // we ship hard-codes the same literal: rely on the global
        // static set of known ids to avoid leaking memory here.
        match self.descriptor().id.as_str() {
            "ron"        => "ron",
            "json"       => "json",
            "toml"       => "toml",
            "yaml"       => "yaml",
            "properties" => "properties",
            _            => "unknown",
        }
    }
}
