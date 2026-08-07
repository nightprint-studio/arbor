//! Canonical entry point for `bennu-core`'s public API.
//!
//! Workspace convention: call sites (in `bennu-be`) reach this crate's surface
//! through `bennu_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub use crate::state::BennuState;

pub use crate::config::{
    load as load_config, load_workspaces, save as save_config, save_workspaces, BennuConfig,
    BennuWorkspace, BennuWorkspaces, CargoConfig, CustomLspServer, LibraryBeansConfig, LspConfig,
    ProjectSession,
};
