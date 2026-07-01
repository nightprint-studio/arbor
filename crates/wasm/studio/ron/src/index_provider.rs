//! The caller-supplied index/scanner seam for RON's self-serving
//! F12 (cross-ref rename) and F13 (project-wide bulk edit).
//!
//! RON keeps a **hand-written** backend with its OWN project-wide
//! `rename_preview` / `bulk_edit_preview` (RON stays special — it does
//! not delegate to `core::refactor` the way the simple formats and JSON
//! do). But the repo scanner + persistent studio index live in the
//! launcher (`crate::studio::{index, scan_repo}`) and move to
//! `arbor-studio-api` in Stage 4 — the format crate must NOT name them
//! (the DAG forbids it).
//!
//! So the backend holds an `Arc<dyn RonIndexProvider>` injected at
//! construction. The launcher implements it against its index/scanner;
//! tests + future callers can pass a no-op or fixture provider.

use std::sync::Arc;

use arbor_studio_types::prelude::StudioResult;

/// One RON definition site the project-wide cross-ref index surfaced.
/// Mirrors the launcher's `CrossRefDef` (RON slice) but carries only the
/// fields the backend's rename-site builder needs — keeps the seam narrow.
#[derive(Debug, Clone)]
pub struct RonRenameDef {
    /// The matched id/name value at this definition.
    pub id_value:      String,
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    /// AST path to the definition value.
    pub def_path:      Vec<String>,
    /// Field name carrying the id (`id`, `name`, …).
    pub def_field:     String,
}

/// One RON usage (reference) site the project-wide cross-ref index
/// surfaced. Mirrors the launcher's `UsageMatch` (RON slice).
#[derive(Debug, Clone)]
pub struct RonRenameUsage {
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name:     String,
    /// AST path to the reference value.
    pub field_path:    Vec<String>,
    /// Last path segment / key name.
    pub key_name:      String,
}

/// The aggregated rename inputs for one rename target.
#[derive(Debug, Clone, Default)]
pub struct RonRenameInputs {
    /// Definitions whose id/name equals the rename target.
    pub defs:           Vec<RonRenameDef>,
    /// References to the rename target.
    pub usages:         Vec<RonRenameUsage>,
    /// Definitions whose id/name equals the proposed new value — used to
    /// surface post-rename namespace collisions (FROZEN F12, sticky
    /// warning, not a hard block). Empty when no `new_value_hint` given.
    pub collision_defs: Vec<RonRenameDef>,
}

/// One RON file the project-wide scan surfaced. Mirrors the launcher's
/// `StudioFileEntry` (filtered to the RON slice).
#[derive(Debug, Clone)]
pub struct ScanFile {
    pub absolute_path: String,
    pub relative_path: String,
    pub name:          String,
    pub size_bytes:    u64,
    /// `true` when the file matches an `excludes` glob — the cross-ref /
    /// bulk-edit scanners skip excluded files.
    pub excluded:      bool,
}

/// Caller-supplied access to the repo's RON cross-ref index + file scan.
///
/// All methods are synchronous (the backend wraps them in
/// `spawn_blocking`) and must be `Send + Sync` so the backend can hold an
/// `Arc<dyn RonIndexProvider>`.
pub trait RonIndexProvider: Send + Sync {
    /// Refresh + aggregate the RON slice of the cross-ref index for
    /// `repo_root`, returning the definition + usage sites the
    /// rename-site builder consumes. `old_value` filters defs/usages to
    /// the rename target; `new_value_hint` (when present and distinct)
    /// drives the collision list.
    fn rename_inputs(
        &self,
        repo_root:      &str,
        old_value:      &str,
        new_value_hint: Option<&str>,
    ) -> StudioResult<RonRenameInputs>;

    /// Walk every RON file under `repo_root` (best-effort; parse/IO
    /// failures are skipped by the caller). Used by project-wide F13 and
    /// `list_files`.
    fn scan_files(&self, repo_root: &str) -> StudioResult<Vec<ScanFile>>;
}

/// A no-op provider: rename returns empty inputs, scan returns no files.
/// Used by [`crate::backend`] (the provider-free factory) and tests that
/// exercise the active-doc paths without a repo.
pub struct NoIndexProvider;

impl RonIndexProvider for NoIndexProvider {
    fn rename_inputs(
        &self,
        _repo_root:      &str,
        _old_value:      &str,
        _new_value_hint: Option<&str>,
    ) -> StudioResult<RonRenameInputs> {
        Ok(RonRenameInputs::default())
    }

    fn scan_files(&self, _repo_root: &str) -> StudioResult<Vec<ScanFile>> {
        Ok(Vec::new())
    }
}

/// Convenience alias for the injected provider handle.
pub type SharedIndexProvider = Arc<dyn RonIndexProvider>;
