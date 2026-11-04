//! Canonical entry point for `bennu-npm`'s public API.

pub use crate::manifest::{
    is_package_manifest, parse, Dependency, DependencyKind, PackageManifest, Script,
};
pub use crate::lock::{lockfile_in, read as read_lock, Edge, Lock, LockFormat, Package as LockedPackage};
pub use crate::workspace::{glob_matches, members as workspace_members};
pub use crate::registry::{
    bump, Bump, cache_path, is_fresh, latest_url, package_manager_for, range_admits, read_cache, write_cache,
    PackageManager, REGISTRY,
};
