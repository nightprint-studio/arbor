//! `bennu-deps` — what a project depends on, and who decided each answer.
//!
//! ## Why this is not "list the jars"
//!
//! The resolved classpath is a flat list of paths in `~/.m2`. It says what the compiler is being
//! handed, which is worth knowing, and nothing else: not which module asked for it, not which
//! scope it is in, not whether anyone declared it or something dragged it in — and above all not
//! *where the version came from*, which is the question a dependency panel exists to answer.
//!
//! In a real project a version is almost never written next to the dependency that uses it. It is
//! a `${property}` defined three poms up, or a `<dependencyManagement>` entry in the parent, or
//! nothing at all because a transitive dependency chose it. Answering "which version am I getting
//! and who decided" means reading the poms the way Maven reads them.
//!
//! So both halves are here, and the panel shows them next to each other:
//!
//! | Source | Answers |
//! |---|---|
//! | the poms ([`pom`], [`graph`]) | modules, scopes, `optional`, profiles, the version and **its origin**, where it is written |
//! | the resolved classpath ([`repo`]) | whether it is actually there, and what came in behind it |
//!
//! ## Two ecosystems, one report
//!
//! [`graph::read`] answers for a Maven reactor and [`cargo::read`] for a Cargo workspace, and both
//! produce the same [`model::Report`] — one panel, one set of questions. The vocabularies differ
//! (`scope` against `kind`, `groupId` against a crate name) and [`model`] documents exactly how they
//! line up and which two fields belong to one ecosystem only.
//!
//! ## Nothing is executed
//!
//! No Maven, no Cargo, no network, no build. The Maven classpath is whatever the index service
//! already resolved; the Cargo versions come from `Cargo.lock` — this crate reads files and matches
//! names. A project that has never been built still lists its dependencies correctly; it just cannot
//! say which of them resolved.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_deps::prelude::...`.

// One `pom.xml`, read structurally.
pub mod pom;
// The Cargo producer of the same report — see [`model`] for how the two ecosystems share a shape.
pub mod cargo;
// The reactor, assembled: inheritance, properties, management, and the jar match.
pub mod graph;
// A local-repository jar path, read back as a coordinate.
pub mod repo;
// What a dependency is once every question about it has been answered.
pub mod model;
pub mod prelude;
