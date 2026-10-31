//! Canonical entry point for `bennu-gradle`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_gradle::prelude::...`.

// The whole build, read.
pub use crate::project::{is_gradle_root, read, read_properties, GradleModule, GradleProject};

// What a build script declares.
pub use crate::build_file::{
    parse_coordinate, BuildInfo, Env, JavaLevel, PluginDecl, RawDep, VersionOrigin,
};

// The settings and the version catalog.
pub use crate::catalog::{Catalog, CatalogLibrary, CatalogPlugin};
pub use crate::settings::{project_path, Settings};

// The Gradle cache on this machine.
pub use crate::cache::{
    coord_of as cache_coord_of, gradle_user_home, is_fixed_version, jars_of, modules_dir,
    resolve_declared, versions_of, Resolution,
};

// The tasks a project offers.
pub use crate::tasks::{tasks_for, Task};
