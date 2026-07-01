//! Schema-view DTOs — the wire shape the FE schema panel consumes.
//!
//! These describe a Rust-source (or JSON-Schema) type reflection: the
//! resolved type expressions, type definitions, root candidates, and
//! per-load stats. The *loader logic* (syn-based `.rs` walking, JSON
//! Schema mapping) stays in the format crates (`arbor-studio-ron` /
//! `arbor-studio-json`); this module owns only the data they produce.

use std::collections::BTreeMap;

use serde::Serialize;

/// A resolved type expression. The `ResolvedType` is what the UI uses to
/// understand any node it sees. Generics are concretised at every use
/// site, so `Option<Vec<Server>>` becomes a 3-level nested `ResolvedType`,
/// not an unresolved generic.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedType {
    /// Rust primitive: `u8..u128`, `i8..i128`, `f32`, `f64`, `bool`, `char`,
    /// `String`, `&str`, `()`.
    Primitive { name: String },
    /// `Option<T>`.
    Option { inner: Box<ResolvedType> },
    /// `Vec<T>`, `[T; N]`, `&[T]`, `VecDeque<T>`, etc. — any homogeneous list.
    Vec { inner: Box<ResolvedType> },
    /// `HashMap<K, V>`, `BTreeMap<K, V>`.
    Map { key: Box<ResolvedType>, value: Box<ResolvedType> },
    /// Tuple `(T1, T2, …)`.
    Tuple { items: Vec<ResolvedType> },
    /// A named type defined inside the current crate. The `path` is the
    /// canonical fully-qualified path (`crate::server::Server`); look it up
    /// in `Schema::types` for the definition.
    Named { path: String },
    /// A type from another crate or std (`tokio::net::TcpStream`,
    /// `std::time::Duration`, …) we don't resolve.
    External { path: String },
    /// We tried to resolve and failed. Surfaced to the UI as a yellow badge.
    Unknown { hint: String },
}

/// Definition of a user-defined type from the indexed crate.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeDef {
    Struct {
        /// Canonical name (`Server`).
        name: String,
        /// `Foo { a: T, b: U }` → named fields. `Foo(T, U)` → tuple fields
        /// (synthetic names "0", "1", …). `Foo;` → empty.
        fields: Vec<FieldDef>,
        /// True for tuple structs `struct Foo(T, U);` — RON renders these
        /// without field names.
        tuple_like: bool,
    },
    Enum {
        name: String,
        variants: Vec<VariantDef>,
    },
    /// `type Foo = Bar;` — flattened to the aliased type at lookup time.
    Alias {
        name: String,
        target: ResolvedType,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    /// SERIALIZED field name — the string the FE walker matches against
    /// the in-source key. This is the result of applying the field's
    /// `#[serde(rename = "...")]` (highest priority) or the struct
    /// container's `#[serde(rename_all = "...")]` (case-converted from
    /// the Rust identifier) to the original Rust ident. Falls back to
    /// the bare Rust ident when neither is set.
    pub name:     String,
    pub ty:       ResolvedType,
    /// Additional accepted names from `#[serde(alias = "...")]` (may
    /// repeat) **plus** the Rust source identifier when it differs
    /// from `name` (so a doc that hand-typed the Rust name still
    /// resolves correctly).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases:  Vec<String>,
    /// `#[serde(default)]`, `#[serde(default = "...")]` → the field has a
    /// default and is therefore allowed to be absent.
    pub has_default: bool,
    /// `#[serde(skip_serializing_if = "...")]` → may be absent in serialised
    /// output for optional-like values.
    pub skip_if_default: bool,
    /// `#[serde(flatten)]` — fields are inlined into the parent.
    pub flatten: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariantDef {
    pub name:       String,
    /// `Foo` → Unit; `Foo(T, U)` → Tuple; `Foo { a: T, b: U }` → Struct.
    pub shape:      VariantShape,
    pub fields:     Vec<FieldDef>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VariantShape {
    Unit,
    Tuple,
    Struct,
}

/// The complete schema returned to the frontend after a successful load.
#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    /// Canonical path of the root type the user selected (`crate::Config`).
    pub root_type:    String,
    /// `Config` — last segment of `root_type`.
    pub root_name:    String,
    /// Absolute path to the `Cargo.toml` we discovered.
    pub crate_manifest: String,
    /// Crate name as declared in the manifest (`[package].name`). Used only
    /// for display.
    pub crate_name:   String,
    /// All resolved type definitions reachable from the root, keyed by
    /// canonical path (`crate::server::Server`). Includes the root.
    pub types:        BTreeMap<String, TypeDef>,
    /// Counts surfaced in the schema panel of the modal.
    pub stats:        SchemaStats,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SchemaStats {
    pub resolved:   usize,
    pub external:   usize,
    pub unknown:    usize,
}

/// Items that can be picked as a "root type". One per public/private
/// struct/enum in the file the user opened. Stable order = source order.
#[derive(Debug, Clone, Serialize)]
pub struct RootCandidate {
    pub name:          String,
    /// Canonical crate-relative path of this type (e.g.
    /// `crate::server::Server`).
    pub canonical_path: String,
    pub kind:          CandidateKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrateProbe {
    pub crate_manifest:    String,
    pub crate_name:        String,
    /// All struct/enum items defined in the FILE the user picked. Either of
    /// them is a valid root; the dropdown is populated from this list.
    pub root_candidates:   Vec<RootCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeSource {
    pub canonical_path: String,
    pub name:           String,
    pub kind:           CandidateKind,
    pub source:         String,
}
