//! Canonical entry point for `sitta-core`'s public API.
//!
//! Workspace convention: call sites (in `sitta-be`) reach this crate's surface
//! through `sitta_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub use crate::state::SittaState;

pub use crate::config::{
    ExplorerColumnConfig, ExplorerSavedSearch, ExplorerSectionConfig, SittaConfig,
};
