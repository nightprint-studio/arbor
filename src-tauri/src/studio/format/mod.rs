//! Studio format backbone — re-export façade over the extracted crates.
//!
//! The DTOs + descriptor + errors moved out of the launcher in Stage 1
//! of the studio extraction into `arbor-studio-types`; the
//! `StudioFormatBackend` trait into `arbor-studio-core`. This module
//! re-publishes the DTO/descriptor/error surface under the historical
//! `crate::studio::format::{descriptor,errors,types}` paths so the IPC
//! layer keeps compiling unchanged. The `StudioRegistry` + the per-format
//! wiring moved to `arbor-studio-api` in Stage 4.
//!
//! See FROZEN F17 in `project_studio_multi_format.md` for the design
//! contract: per-format Tauri commands are forbidden; every format
//! implements `StudioFormatBackend`, registers itself in
//! `AppState.studio_registry`, and the UI consults its
//! `FormatDescriptor` to decide which capabilities are available.

/// `FormatDescriptor` + sub-enums (now in `arbor-studio-types`).
pub mod descriptor {
    pub use arbor_studio_types::descriptor::*;
}

/// `StudioError` / `StudioResult` / `to_ipc` (now in `arbor-studio-types`).
pub mod errors {
    pub use arbor_studio_types::errors::*;
}

/// Format-agnostic DTOs (now in `arbor-studio-types`). Re-exports both
/// the trait/IPC shapes and the schema-view DTOs so the historical
/// `studio::format::types::{Schema, CrateProbe, TypeSource, …}` paths
/// keep resolving.
pub mod types {
    pub use arbor_studio_types::dto::*;
}

/// `StudioRegistry` (now in `arbor-studio-api`). Re-exported here so the
/// historical `crate::studio::format::StudioRegistry` path resolves.
pub use arbor_studio_api::StudioRegistry;
