//! Canonical entry point for `bennu-cargo`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_cargo::prelude::...`.

// A manifest, read.
pub use crate::manifest::{Entry, InlineKey, Item, Manifest, TableSpan, ROOT_TABLE};

// What a manifest may contain — read by both validation and completion.
pub use crate::schema::{
    canonical_path, crate_types, editions, is_dependency_table, lint_keys, lint_levels, table_def,
    KeyDef, Openness, TableDef, ValueKind, DEP_KEYS, HEADER_SUGGESTIONS, TABLES,
};

// The dependencies a manifest declares.
pub use crate::deps::{declared, DeclaredDep, DepKind};

// "Is this manifest right."
pub use crate::validate::{validate, validate_file, Context as ValidateContext};

// "What can I type here."
pub use crate::complete::{complete, spot_at, Catalog, Context as CompleteContext, Spot};

// The crate graph.
pub use crate::workspace::{
    expand_members, read as read_workspace, CargoCrate, CargoFeature, CargoTarget, CargoWorkspace,
};

// Where cargo keeps things on this machine.
pub use crate::home::{cargo_home, registry_dirs};

// The crates.io index. The fetch is the caller's — see the module.
pub use crate::registry::{
    cache_path as index_cache_path, index_path, index_url, is_fresh as index_is_fresh, is_release,
    latest_release, parse_index, read_cache as read_index_cache, requirement_admits,
    write_cache as write_index_cache, IndexVersion, CRATES_IO_INDEX,
};

// The cargo commands, and how one becomes an argv.
pub use crate::commands::{
    argv, command, display as display_command, CommandDef, Invocation, TargetSelector, COMMANDS,
};
