//! Canonical entry point for `arbor-core`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers should reach types and
//! functions through `arbor_core::prelude::...` (or a single
//! `use arbor_core::prelude::*;` at the top of a module) rather than
//! through the per-feature submodule paths. The submodules stay `pub` for
//! rustdoc navigation, but call sites should go through here.

pub use crate::app_ctx::AppCtx;
pub use crate::error::{CoreError, Result};
pub use crate::http::{client, client_builder, DEFAULT_TIMEOUT, USER_AGENT};
pub use crate::paths::{
    arbor_cache_dir, arbor_config_dir, arbor_config_path, arbor_data_dir,
    try_arbor_config_path,
};
