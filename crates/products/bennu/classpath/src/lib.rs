//! `bennu-classpath` — JVM-free bytecode reading for Bennu.
//!
//! **Leaf crate** (docs §10): no Bennu dependencies, only the off-the-shelf
//! bytecode stack (`cafebabe` + `zip` + `jimage-rs`) plus a homegrown decoder for
//! the JVM `Signature` attribute (JVMS §4.7.9.1) — the one piece no Rust crate does
//! (docs §4). Ported verbatim from the proven `bennu-spike-bytecode`.
//!
//! Two container formats sit behind **one** [`ClassSource`] trait and **one**
//! member-index API ([`ClassMeta`]): the only real difference between them is the
//! resource path — `java/lang/String.class` in a ZIP jar vs
//! `/java.base/java/lang/String.class` in a jimage (docs §10). Impls:
//!
//! - [`DirSource`] — a plain directory of `.class` files (e.g. `target/classes`).
//! - [`JarSource`] — a ZIP jar (rt.jar on JDK 8, dependency jars).
//! - [`JimageSource`] — the JDK 9+ `lib/modules` jimage.
//!
//! On top of the byte-container layer sit two views of a class:
//!
//! - [`meta::ClassMeta`] — the human-readable rendering (one string per member).
//! - [`members::ClassMembers`] — the **shared-seam** structured view: `TypeRef` /
//!   `Member` / `MemberIndex`, with generics kept structured so `bennu-java` and
//!   `bennu-intel` can resolve member-access with type-argument carry-through. The
//!   [`jdk::resolve_jdk_classpath`] entry point turns a Java language level into a
//!   ready `ClassSource` over the matching installed JDK's bootclasspath.
//!
//! Dependency-jar bytecode from `~/.m2` layers in behind the **same** `ClassSource` /
//! `MultiSource` / member-index API (docs §10): [`maven::resolve_maven_classpath`]
//! runs `mvn dependency:build-classpath` (cached by pom mtime), opens each resolved
//! dep jar as a [`JarSource`], and [`maven::MavenClasspath::augment`] chains them
//! behind the JDK — so completion reaches framework/library types (Spring, servlet,
//! Hibernate…), and a project with no resolvable deps degrades to JDK-only.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_classpath::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub mod jdk;
pub mod maven;
pub mod members;
pub mod meta;
pub mod prelude;
pub mod sig;
pub mod source;
