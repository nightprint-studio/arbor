//! The resolver seam the type-walk consumes, plus the minimal member shapes it
//! resolves against.
//!
//! These mirror the shared Bennu seam (docs §10): `TypeRef` carries generics
//! (caveat C2, generics carry-through), and [`ClassMembers`]/[`Member`] are the
//! resolved member metadata a class exposes. `bennu-classpath` produces the same
//! shape from bytecode; `bennu-intel` unifies the two at the boundary. We keep a
//! local, minimal copy here so `bennu-java` depends only on the [`TypeResolver`]
//! trait, not on any concrete member-index implementation.

use serde::{Deserialize, Serialize};

/// A resolved type reference carrying its generic arguments (seam caveat C2:
/// generics carry-through). `binary_name` is a slash-separated JVM binary name, e.g.
/// `java/util/ArrayList`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRef {
    /// The JVM binary name (`java/util/ArrayList`).
    pub binary_name: String,
    /// Actual generic arguments, in declaration order (empty when raw / non-generic).
    pub type_args: Vec<TypeRef>,
}

impl TypeRef {
    /// A type reference with no generic arguments.
    pub fn simple(binary_name: impl Into<String>) -> Self {
        Self { binary_name: binary_name.into(), type_args: Vec::new() }
    }
}

/// Whether a member is a method or a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberKind {
    Method,
    Field,
}

/// Member visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Protected,
    Private,
    Package,
}

/// One method or field of a class, with resolved types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub kind: MemberKind,
    /// For a field, its type; for a method, its return type.
    pub return_type: TypeRef,
    /// Parameter types (methods only; empty for fields).
    pub params: Vec<TypeRef>,
    pub is_static: bool,
    pub visibility: Visibility,
    /// A readable, best-available signature rendering (for completion `detail`).
    pub raw_signature: String,
}

/// The resolved members of a class, plus its supertype links so the walk can pick up
/// inherited members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassMembers {
    /// Binary name of the superclass, if any (`java/lang/Object` for most).
    pub superclass: Option<String>,
    /// Binary names of directly-implemented interfaces.
    pub interfaces: Vec<String>,
    pub methods: Vec<Member>,
    pub fields: Vec<Member>,
}

/// The resolver the inference walk consumes. Provided by the caller — in the real
/// build, backed by `bennu-classpath` (JDK + jars) plus the project source index.
pub trait TypeResolver {
    /// Members of a class by binary name (`java/util/ArrayList`). `None` when the
    /// class isn't on the resolvable classpath (a normal, non-fatal state).
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers>;

    /// Resolve a simple type name (`ArrayList`) to a binary name, using the file's
    /// imports for disambiguation. `None` when unresolvable.
    fn resolve_simple_name(&self, name: &str, imports: &[crate::symbols::Import])
        -> Option<String>;
}
