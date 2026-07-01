//! Api-side index/scanner providers for `arbor-studio-ron` /
//! `arbor-studio-json`.
//!
//! The extracted RON/JSON crates keep their hand-written self-serving
//! F12/F13 but can't name the api's repo scanner / persistent cross-ref
//! index (the crate DAG forbids it). So each backend holds an
//! `Arc<dyn …IndexProvider>`; these adapters wire them to
//! `crate::{scanner, index, refactor_glue}`.

use arbor_studio_core::prelude::{RenameDefInput, RenameUsageInput};
use arbor_studio_json::prelude::{JsonIndexProvider, ScanFile};
use arbor_studio_ron::prelude::{
    RonIndexProvider, RonRenameDef, RonRenameInputs, RonRenameUsage, ScanFile as RonScanFile,
};
use arbor_studio_types::prelude::StudioResult;

use crate::scanner::{self, StudioFileKind};
use crate::{index, refactor_glue};

/// JSON index/scanner provider backed by the api studio index.
pub struct LauncherJsonIndex;

impl JsonIndexProvider for LauncherJsonIndex {
    fn rename_inputs(
        &self,
        repo_root: &str,
        old_value: &str,
    ) -> StudioResult<(Vec<RenameDefInput>, Vec<RenameUsageInput>)> {
        let kinds = [StudioFileKind::Json];
        // Refresh the JSON slice of the index — keeps RON's slice
        // untouched. Fall back to whatever's already on disk if refresh
        // fails (filesystem hiccup, permission).
        let idx = match index::refresh_for(repo_root, &kinds, None) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!(
                    "rename_inputs (json): index refresh failed, falling back to fresh scan ({e})"
                );
                index::load(repo_root)
            }
        };
        let defs   = refactor_glue::collect_rename_defs(&idx, &kinds);
        let usages = refactor_glue::collect_rename_usages(&idx, old_value, &kinds);
        Ok((defs, usages))
    }

    fn scan_files(&self, repo_root: &str) -> StudioResult<Vec<ScanFile>> {
        let files = scanner::scan_repo(repo_root, &[StudioFileKind::Json])?;
        Ok(files
            .into_iter()
            .map(|f| ScanFile {
                absolute_path: f.absolute_path,
                relative_path: f.relative_path,
                name:          f.name,
                size_bytes:    f.size_bytes,
                excluded:      f.excluded,
            })
            .collect())
    }
}

/// RON index/scanner provider backed by the api studio index.
pub struct LauncherRonIndex;

impl RonIndexProvider for LauncherRonIndex {
    fn rename_inputs(
        &self,
        repo_root:      &str,
        old_value:      &str,
        new_value_hint: Option<&str>,
    ) -> StudioResult<RonRenameInputs> {
        let kinds = [StudioFileKind::Ron];
        // Smart reindex of the RON slice — keeps JSON's slice untouched.
        // Fall back to whatever's already on disk if refresh fails.
        let idx = match index::refresh_for(repo_root, &kinds, None) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!(
                    "rename_inputs (ron): index refresh failed, falling back to fresh scan ({e})"
                );
                index::load(repo_root)
            }
        };

        let defs: Vec<RonRenameDef> = index::aggregate_cross_refs_for(&idx, &kinds)
            .into_iter()
            .filter(|d| d.id_value == old_value)
            .map(|d| RonRenameDef {
                id_value:      d.id_value,
                absolute_path: d.absolute_path,
                relative_path: d.relative_path,
                file_name:     d.file_name,
                def_path:      d.def_path,
                def_field:     d.def_field,
            })
            .collect();

        let usages: Vec<RonRenameUsage> = index::aggregate_usages_for(&idx, old_value, &kinds)
            .into_iter()
            .map(|u| RonRenameUsage {
                absolute_path: u.absolute_path,
                relative_path: u.relative_path,
                file_name:     u.file_name,
                field_path:    u.field_path,
                key_name:      u.key_name,
            })
            .collect();

        // Collision defs: every existing def whose value equals a distinct
        // `new_value_hint`. Empty when no hint or hint == old_value.
        let collision_defs: Vec<RonRenameDef> = match new_value_hint {
            Some(hint) if !hint.is_empty() && hint != old_value => {
                index::aggregate_cross_refs_for(&idx, &kinds)
                    .into_iter()
                    .filter(|d| d.id_value == hint)
                    .map(|d| RonRenameDef {
                        id_value:      d.id_value,
                        absolute_path: d.absolute_path,
                        relative_path: d.relative_path,
                        file_name:     d.file_name,
                        def_path:      d.def_path,
                        def_field:     d.def_field,
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        Ok(RonRenameInputs { defs, usages, collision_defs })
    }

    fn scan_files(&self, repo_root: &str) -> StudioResult<Vec<RonScanFile>> {
        let files = scanner::scan_repo(repo_root, &[StudioFileKind::Ron])?;
        Ok(files
            .into_iter()
            .map(|f| RonScanFile {
                absolute_path: f.absolute_path,
                relative_path: f.relative_path,
                name:          f.name,
                size_bytes:    f.size_bytes,
                excluded:      f.excluded,
            })
            .collect())
    }
}
