//! Canonical entry point for `bennu-maven`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_maven::prelude::...`. A host needs three things: the extension to register, the offline
//! resolver for the dependency tier, and the repository when it wants to say where it looked.

// What a host registers.
pub use crate::ext::MavenExtension;

// The classpath, without Maven.
pub use crate::resolve::{reactor, resolve as resolve_offline, Resolution};

// Where the repository is, and what a coordinate is called inside it.
pub use crate::repo::{compare_versions, local_repository, sort_versions_desc, Coord, LocalRepo};

// What is in it.
pub use crate::catalog::{Artifact, Catalog};

// The bundled table, for a machine whose repository cannot answer yet.
pub use crate::known::{describe as describe_coordinate, LIBRARIES, PLUGINS};

// Inheritance folded in — for a caller that wants a pom's real versions rather than its written ones.
pub use crate::effective::{effective_of_buffer, Effective, Managed, PomReader, Resolved};

// The editor answers, for a caller that has its own context rather than the extension's.
pub use crate::blocks::{block_at, blocks, Block, BlockKind};
pub use crate::check::diagnostics as pom_diagnostics;
pub use crate::complete::completions as pom_completions;
pub use crate::doc::Doc as PomDoc;
pub use crate::env::PomEnv;
pub use crate::explain::{hover as pom_hover, navigate as pom_navigate};
