//! `bennu-gradle` — what a Gradle build IS, read off its files.
//!
//! The Gradle counterpart of `bennu-cargo` and `bennu-maven`: a leaf crate that reads, never runs.
//! Gradle builds are programs (Groovy or Kotlin), so what is read is the declarative subset every
//! real build file is made of — see [`script`] for how, and [`build_file`] for what is and is not
//! understood. Anything that needs the program to *run* (a dependency produced by a loop, a
//! convention plugin, the full transitive graph) is Gradle's to answer, and the backend asks it.
//!
//! ## Public API: use the [`prelude`]

// A script, masked and scanned: blocks, statements, string literals.
pub mod script;
// `gradle/libs.versions.toml`.
pub mod catalog;
// `settings.gradle[.kts]` — the subprojects.
pub mod settings;
// One `build.gradle[.kts]` — plugins, dependencies, the Java level.
pub mod build_file;
// The whole build assembled from the above.
pub mod project;
// The jars the Gradle cache already holds.
pub mod cache;
// The tasks a project offers.
pub mod tasks;
pub mod prelude;
