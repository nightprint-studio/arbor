//! Canonical entry point for `picus-core`'s public API.
//!
//! Workspace convention: call sites (in `picus-be`) reach this crate's surface
//! through `picus_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub use crate::state::PicusState;

pub use crate::config::{
    InsertionRule, PicusConfig, PicusEncodingConfig, PicusGenerationConfig, PicusQueryConfig,
    PicusWritingConfig, DEFAULT_ROW_LIMIT, ROW_LIMIT_RANGE,
};

pub use crate::connections::{
    connections_path, load_connections, save_connections, ConnectionFile, SessionPool,
};

pub use crate::digest::digest;

pub use crate::schema::SchemaCache;
pub use crate::scripts::{cache_key, CachedSource, ScriptCache, ScriptSnapshot};
