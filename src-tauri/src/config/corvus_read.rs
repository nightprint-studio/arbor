//! Thin, read-only access to corvus-be's owned `corvus/config.toml`.
//!
//! corvus-be is the **sole writer** of the corvus product config (the diff /
//! status / recovery / cache / pipelines / … sections — see
//! `crates/products/corvus/be/src/corvus_config.rs`). A handful of *shell* code paths
//! still need to read one of those values in-process:
//! - the in-process recovery snapshotter (`crate::git::recovery`) needs the
//!   snapshot policy;
//! - the Studio index code needs the `use_index` flag;
//! - cache eviction needs `close_repo_on_evict`;
//! - the pipeline engine reads its concurrency cap at construction.
//!
//! These are **partial-struct direct reads** of the TOML file — the same
//! precedent as the per-repo `.arbor/config.toml` reads (`integrations`,
//! `ipc::corvus::ide`). The shell NEVER writes this file; that would create a
//! cross-process dual-writer. Each caller applies its own default when the file
//! or section is absent.

use std::path::PathBuf;

use arbor_core::prelude::{product_path, PRODUCT_CORVUS};
use serde::de::DeserializeOwned;

/// Absolute, profile-resolved path of the corvus product config file.
pub fn corvus_config_path() -> PathBuf {
    product_path(PRODUCT_CORVUS, "config.toml")
}

/// Deserialize one top-level section of `corvus/config.toml` into `T`.
/// Returns `None` when the file is missing/unreadable, the section is absent,
/// or it fails to deserialize — callers supply their own default.
pub fn section<T: DeserializeOwned>(name: &str) -> Option<T> {
    let text = std::fs::read_to_string(corvus_config_path()).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table.get(name)?.clone().try_into().ok()
}
