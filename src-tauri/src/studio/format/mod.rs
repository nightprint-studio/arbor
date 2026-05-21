//! Studio format backbone — trait + registry + descriptor + the one
//! set of Tauri commands shared by every per-format implementation
//! (RON / JSON / TOML / YAML / .properties).
//!
//! See FROZEN F17 in `project_studio_multi_format.md` for the design
//! contract. The short version: per-format Tauri commands are
//! forbidden; every format implements `StudioFormatBackend`, registers
//! itself in `AppState.studio_registry`, and the UI consults its
//! `FormatDescriptor` to decide which capabilities are available.

pub mod backend;
pub mod commands;
pub mod descriptor;
pub mod errors;
pub mod properties_codec;
pub mod registry;
pub mod types;

pub use registry::StudioRegistry;
