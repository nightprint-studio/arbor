//! Canonical entry point for `bennu-classpath`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_classpath::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

// The human-readable rendering of a class (one string per member).
pub use crate::meta::{parse_class_meta, ClassMeta, MemberMeta};

// The shared-seam structured member index: TypeRef / Member / ClassMembers /
// MemberIndex, plus the ClassSource→MemberIndex adapter.
pub use crate::members::{
    parse_class_members, ClassFlags, ClassMembers, Member, MemberIndex, MemberKind,
    SourceMemberIndex, TypeRef, Visibility,
};

// JDK bootclasspath resolution by language level, the chained multi-source, the JDK-home
// locator (JAVA_HOME for the build/run shell-out), the resolution-status probe (FE JDK
// diagnostics), and the user-configured extra-JDK-homes setter.
pub use crate::jdk::{
    find_jdk_home, jdk_status, resolve_jdk_classpath, resolve_jdk_sources, set_extra_jdk_homes,
    JdkStatus, MultiSource,
};

// Java SOURCE containers (`.java` text with bodies) for the "go to source" view — the JDK's
// `src.zip` and a dependency's `-sources.jar`, distinct from the bytecode `ClassSource`.
pub use crate::sources::JavaSourceZip;

// Dependency-jar sourcing from ~/.m2 via Maven's build-classpath (cached by pom
// mtime), layered behind the JDK through the same MultiSource.
pub use crate::maven::{
    resolve_maven_classpath, source_from_jars, MavenClasspath, MavenClasspathCache, MavenResolveOpts,
};

// The container abstraction + its three impls.
pub use crate::source::{ClassSource, DirSource, JarSource, JimageSource};

// The homegrown JVMS §4.7.9.1 Signature decoder (types + entry points).
pub use crate::sig::{
    parse_class as parse_class_signature, parse_field as parse_field_signature,
    parse_method as parse_method_signature, ClassSig, ClassType, MethodSig, SigParser, TypeArg,
    TypeParam, TypeSig,
};
