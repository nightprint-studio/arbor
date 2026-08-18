//! Canonical entry point for `bennu-toml`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_toml::prelude::...`. The submodule stays `pub` for rustdoc navigation, but the prelude
//! is the canonical call-site path.

pub use crate::manifest::{Entry, InlineKey, Item, Manifest, TableSpan, ROOT_TABLE};
