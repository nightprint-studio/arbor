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
    parse_class_members, ClassMembers, Member, MemberIndex, MemberKind, SourceMemberIndex, TypeRef,
    Visibility,
};

// JDK bootclasspath resolution by language level, and the chained multi-source.
pub use crate::jdk::{resolve_jdk_classpath, MultiSource};

// The container abstraction + its three impls.
pub use crate::source::{ClassSource, DirSource, JarSource, JimageSource};

// The homegrown JVMS §4.7.9.1 Signature decoder (types + entry points).
pub use crate::sig::{
    parse_class as parse_class_signature, parse_field as parse_field_signature,
    parse_method as parse_method_signature, ClassSig, ClassType, MethodSig, SigParser, TypeArg,
    TypeParam, TypeSig,
};
