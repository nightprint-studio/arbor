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
    arbor_cache_dir, arbor_config_dir, arbor_config_path, arbor_data_dir, arbor_global_data_dir,
    bennu_config_dir, bennu_config_path, bennu_data_dir, merula_config_dir, merula_config_path,
    merula_data_dir, merula_legacy_sibling_dirs, sitta_config_dir, sitta_config_path,
    sitta_data_dir, try_arbor_config_path, tyto_config_dir, tyto_config_path, tyto_data_dir,
};
pub use crate::profile::{
    active_profile, active_profile_pointer_path, arbor_profile_dir, arbor_profile_path,
    init_active_profile, is_valid_profile_name, marketplace_plugins_dir, product_dir,
    product_dir_for, product_path, profile_dir_for, profile_plugins_dir, profiles_root,
    set_active_profile, try_product_path, DEFAULT_PROFILE, PRODUCT_BENNU, PRODUCT_CORVUS,
    PRODUCT_MERULA, PRODUCT_SITTA, PRODUCT_TYTO,
};
