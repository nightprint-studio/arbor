//! studio — thin launcher shim over `arbor-studio-api`.
//!
//! Stage 4 of the studio extraction moved the registry, the cross-ref
//! scanner, the persistent index, the repo-root config, the project-wide
//! F12/F13 orchestration, and the schema/index-provider wiring into the
//! Tauri-free `arbor-studio-api` crate. What stays in the launcher is the
//! Tauri command/rpc seam (`crate::ipc::studio::*`); this module just
//! re-exports the api surface under the historical
//! `crate::studio::{...}` paths so those handlers keep compiling unchanged.
//!
//! `format` stays here too: it re-publishes the trait/DTO/descriptor/error
//! crates under `crate::studio::format::{backend,descriptor,errors,types}`
//! for the IPC layer (the registry moved to `arbor-studio-api`).

pub mod format;

// ── Scanner + index + config + project-refactor — now in `arbor-studio-api` ──

pub use arbor_studio_api::scanner::{
    find_usages_for, scan_broken_refs_for, scan_cross_refs_for, scan_repo, BrokenRef, CrossRefDef,
    EntrySchema, SchemaHintOriginExt, StudioFileEntry, StudioFileKind, UsageMatch,
};

pub use arbor_studio_api::{config, index, project_refactor};
