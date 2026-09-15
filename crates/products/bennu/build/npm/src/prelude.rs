//! Canonical entry point for `bennu-npm`'s public API.

pub use crate::manifest::{
    is_package_manifest, parse, Dependency, DependencyKind, PackageManifest, Script,
};
pub use crate::registry::{
    cache_path, is_fresh, latest_url, package_manager_for, range_admits, read_cache, write_cache,
    PackageManager, REGISTRY,
};
