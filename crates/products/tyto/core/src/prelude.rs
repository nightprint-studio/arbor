//! Canonical entry point for `tyto-core`'s public API.
//!
//! Workspace convention: call sites (in `tyto-be`) reach this crate's surface
//! through `tyto_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub use crate::state::TytoState;

pub use crate::config::{
    preset_bitrate_kbps, TytoCaptureConfig, TytoConfig, TytoEncodingConfig, TytoFramesConfig,
    TytoOutputConfig,
};
