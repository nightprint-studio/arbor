//! Canonical entry point for `corvus-core`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_core::prelude::...`. The submodules stay `pub` for rustdoc navigation.

pub use crate::state::CorvusState;
