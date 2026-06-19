//! Canonical entry point for `corvus-plugin`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_plugin::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation.

pub use crate::app_ctx::CorvusBeAppCtx;
pub use crate::dispatcher::build_hook_dispatcher;
pub use crate::installer::{corvus_be_api_installer, CorvusBeApiInstaller};
