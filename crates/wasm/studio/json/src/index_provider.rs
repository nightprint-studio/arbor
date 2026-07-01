//! The caller-supplied index/scanner seam for JSON's self-serving
//! F12 (cross-ref rename) and F13 (project-wide bulk edit).
//!
//! JSON keeps a hand-written backend with its OWN project-wide
//! `rename_preview` / `bulk_edit_preview` (unlike the simple formats,
//! which return `Unsupported` and let the api/launcher orchestrate). But
//! the repo scanner + persistent studio index live in the launcher
//! (`crate::studio::{index, scan_repo}`) and move to `arbor-studio-api`
//! in Stage 4 — the format crate must NOT name them (the DAG forbids it).
//!
//! So the backend holds an `Arc<dyn JsonIndexProvider>` injected at
//! construction. The launcher implements it against its index/scanner;
//! tests + future callers can pass a no-op or fixture provider.

use std::sync::Arc;

use arbor_studio_core::prelude::{RenameDefInput, RenameUsageInput};
use arbor_studio_types::prelude::StudioResult;

/// One JSON file the project-wide scan surfaced. Mirrors the launcher's
/// `StudioFileEntry` (filtered to the JSON slice) but carries only the
/// fields the backend needs — keeps the seam narrow.
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

/// Caller-supplied access to the repo's JSON cross-ref index + file scan.
///
/// All methods are synchronous (the backend wraps them in
/// `spawn_blocking`) and must be `Send + Sync` so the backend can hold an
/// `Arc<dyn JsonIndexProvider>`.
pub trait JsonIndexProvider: Send + Sync {
    /// Refresh + aggregate the JSON slice of the cross-ref index for
    /// `repo_root`, returning the definition + usage inputs the
    /// `core::refactor` rename-site builder consumes. `old_value` filters
    /// the usages to references of the rename target.
    fn rename_inputs(
        &self,
        repo_root: &str,
        old_value: &str,
    ) -> StudioResult<(Vec<RenameDefInput>, Vec<RenameUsageInput>)>;

    /// Walk every JSON file under `repo_root` (best-effort; parse/IO
    /// failures are skipped by the caller). Used by project-wide F13 and
    /// `list_files`.
    fn scan_files(&self, repo_root: &str) -> StudioResult<Vec<ScanFile>>;
}

/// A no-op provider: rename returns empty inputs, scan returns no files.
/// Used by [`crate::backend`] (the schema/provider-free factory) and
/// tests that exercise the active-doc paths without a repo.
pub struct NoIndexProvider;

impl JsonIndexProvider for NoIndexProvider {
    fn rename_inputs(
        &self,
        _repo_root: &str,
        _old_value: &str,
    ) -> StudioResult<(Vec<RenameDefInput>, Vec<RenameUsageInput>)> {
        Ok((Vec::new(), Vec::new()))
    }

    fn scan_files(&self, _repo_root: &str) -> StudioResult<Vec<ScanFile>> {
        Ok(Vec::new())
    }
}

/// Convenience alias for the injected provider handle.
pub type SharedIndexProvider = Arc<dyn JsonIndexProvider>;
