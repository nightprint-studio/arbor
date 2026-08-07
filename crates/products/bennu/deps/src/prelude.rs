//! Canonical entry point for `bennu-deps`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_deps::prelude::...`. In practice a host needs [`read`] and the types it returns;
//! everything below that is reachable for a consumer that wants one pom rather than a project.

// The one call a host makes, per ecosystem.
pub use crate::cargo::read as read_cargo;
pub use crate::graph::read;

// What it returns.
pub use crate::model::{Dependency, Module, Origin, Report, Site, Transitive};

// One pom, for a consumer that has the text and wants the declarations.
pub use crate::pom::{parse as parse_pom, ParentRef, Pom, RawDependency};

// A repository jar path, read back as a coordinate.
pub use crate::repo::{coord_of, JarCoord};
