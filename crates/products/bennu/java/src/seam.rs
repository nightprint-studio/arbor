//! The resolver seam the type-walk consumes, plus the minimal member shapes it
//! resolves against.
//!
//! These mirror the shared Bennu seam (docs §10): `TypeRef` carries generics
//! (caveat C2, generics carry-through), and [`ClassMembers`]/[`Member`] are the
//! resolved member metadata a class exposes. `bennu-classpath` produces the same
//! shape from bytecode; `bennu-intel` unifies the two at the boundary. We keep a
//! local, minimal copy here so `bennu-java` depends only on the [`TypeResolver`]
//! trait, not on any concrete member-index implementation.

use std::sync::Arc;

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
    /// An abstract method (`ACC_ABSTRACT`) — no body; a concrete subclass must implement it.
    #[serde(default)]
    pub is_abstract: bool,
    /// An interface `default` method — a concrete method that satisfies the interface contract.
    #[serde(default)]
    pub is_default: bool,
    /// A `final` method (can't be overridden) or `final` field (can't be reassigned).
    #[serde(default)]
    pub is_final: bool,
    pub visibility: Visibility,
    /// A readable, best-available signature rendering (for completion `detail`).
    pub raw_signature: String,
    /// The checked exceptions a method declares it `throws` (binary names with slashes). Empty for
    /// fields and for methods with no `throws` clause. From bytecode's `Exceptions` attribute (JDK /
    /// library methods) or the source `throws` clause (project methods). `#[serde(default)]` so an
    /// index persisted before this field existed still deserializes (empty).
    #[serde(default)]
    pub throws: Vec<String>,
}

impl Member {
    /// A public, non-static instance **method** with every flag defaulted (`raw_signature` = the
    /// bare name). Chain the fluent setters to adjust. Centralises construction so growing the struct
    /// touches this one spot instead of every call site.
    pub fn method(name: impl Into<String>, return_type: TypeRef, params: Vec<TypeRef>) -> Self {
        let name = name.into();
        let raw_signature = name.clone();
        Self {
            name,
            kind: MemberKind::Method,
            return_type,
            params,
            is_static: false,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Public,
            raw_signature,
            throws: Vec::new(),
        }
    }

    /// A public, non-static **field** of type `ty`, every flag defaulted.
    pub fn field(name: impl Into<String>, ty: TypeRef) -> Self {
        let name = name.into();
        let raw_signature = name.clone();
        Self {
            name,
            kind: MemberKind::Field,
            return_type: ty,
            params: Vec::new(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Public,
            raw_signature,
            throws: Vec::new(),
        }
    }

    /// Mark `static`.
    pub fn stat(mut self) -> Self {
        self.is_static = true;
        self
    }
    /// Mark `abstract`.
    pub fn abstract_(mut self) -> Self {
        self.is_abstract = true;
        self
    }
    /// Mark an interface `default` method.
    pub fn default_(mut self) -> Self {
        self.is_default = true;
        self
    }
    /// Mark `final`.
    pub fn final_(mut self) -> Self {
        self.is_final = true;
        self
    }
    /// Set the visibility.
    pub fn vis(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }
    /// Set the rendered signature (completion `detail`).
    pub fn sig(mut self, raw_signature: impl Into<String>) -> Self {
        self.raw_signature = raw_signature.into();
        self
    }
    /// Set the declared checked exceptions (binary names).
    pub fn throws(mut self, throws: Vec<String>) -> Self {
        self.throws = throws;
        self
    }
}

/// Class-level access flags the checks need (extend-final / extend-record / implement-abstract).
/// Mirrors `bennu_classpath::ClassFlags`; `bennu-intel` copies it across the seam boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassFlags {
    pub is_interface: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_enum: bool,
    pub is_annotation: bool,
    pub is_record: bool,
    pub is_sealed: bool,
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
    /// Class-level access flags. `#[serde(default)]` so a pre-existing persisted index still loads.
    #[serde(default)]
    pub flags: ClassFlags,
}

/// The resolver the inference walk consumes. Provided by the caller — in the real
/// build, backed by `bennu-classpath` (JDK + jars) plus the project source index.
pub trait TypeResolver {
    /// Members of a class by binary name (`java/util/ArrayList`). `None` when the
    /// class isn't on the resolvable classpath (a normal, non-fatal state).
    ///
    /// Returns an `Arc` so a memoizing resolver can hand back a shared handle on a cache
    /// hit — the whole-project reference walk asks for the same hot types (services, DTOs)
    /// tens of thousands of times, and deep-cloning their member lists on each hit is the
    /// dominant cost of the walk; an `Arc` clone is a refcount bump.
    fn members_of(&self, binary_name: &str) -> Option<Arc<ClassMembers>>;

    /// Resolve a simple type name (`ArrayList`) to a binary name, using the file's
    /// imports for disambiguation. `None` when unresolvable.
    fn resolve_simple_name(&self, name: &str, imports: &[crate::symbols::Import])
        -> Option<String>;
}
