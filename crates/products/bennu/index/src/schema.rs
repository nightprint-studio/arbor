//! The multi-source symbol / relation schema (docs §3).
//!
//! Every record carries a `source` tag, so a new fonte (Maven `.m2`, a new config
//! kind) is a new [`Source`] variant feeding the same table — not a rewrite. The
//! records derive rkyv's `Archive`/`Serialize`/`Deserialize` (zero-copy read off the
//! mmap) with `bytecheck` validation on access, exactly as the spike proved.

use rkyv::{Archive, Deserialize, Serialize};

/// What kind of symbol a record is (docs §3).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[rkyv(derive(Debug))]
pub enum SymbolKind {
    Class,
    Interface,
    Enum,
    Record,
    Method,
    Field,
    Param,
    LocalVar,
    Package,
}

/// Which fonte a record came from (docs §3). The tag that makes the index
/// multi-source: `ProjectSource` (tree-sitter'd sources), `JdkBytecode`,
/// `TargetClasses`, `DepBytecode`, and the config-graph sources
/// (`StrutsAction` / `TldTag` / `SpringBean` / …). Extend with a variant; the table
/// stays the same.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[rkyv(derive(Debug))]
pub enum Source {
    /// A symbol parsed from a project `.java` source (mutable, patched per-file).
    ProjectSource,
    /// A symbol read from JDK bytecode (rt.jar / jimage) — immutable.
    JdkBytecode,
    /// A symbol from the project's own compiled `target/classes` — rebuilt on compile.
    TargetClasses,
    /// A symbol from a dependency jar's bytecode — immutable, cached by jar mtime.
    DepBytecode,
    /// A Struts/XWork `<action>` mapping (config-as-symbol).
    StrutsAction,
    /// A JSP TLD tag definition.
    TldTag,
    /// A Spring bean id (`<bean id=…>`).
    SpringBean,
}

/// The kind of a [`Relation`] edge (docs §3). Java type-hierarchy edges plus the
/// config-graph edges that make the XML a first-class language.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[rkyv(derive(Debug))]
pub enum RelationKind {
    Extends,
    Implements,
    Overrides,
    References,
    ActionToClass,
    ActionToResult,
    ResultToView,
    JspInclude,
    JspUsesTaglib,
    BeanIdToImpl,
}

/// One symbol record. Serialized independently into the framed blob; its
/// `simple_name` (and, in other maps, `fqn`) is an fst key → this record's offset.
///
/// `loc_*` is a flattened `FileSpan | ClassRef` (docs §3): a source symbol carries
/// `loc_file` + byte range; a bytecode symbol carries `loc_container` (jar/jimage
/// path) + `loc_class`. Flattened (not an enum) so the archived record stays a plain
/// struct — simplest to slice zero-copy.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct Symbol {
    /// Stable within a partition; also the target of a relation edge.
    pub id: u32,
    pub kind: SymbolKind,
    pub simple_name: String,
    pub fqn: String,
    /// Owning symbol's id (the class for a method/field; `u32::MAX` for a top-level).
    pub owner_id: u32,
    pub source: Source,
    /// The resolved signature (generics from the Signature decoder), rendered.
    pub signature: String,
    pub modifiers: String,
    /// Source location: the file path (empty for a bytecode symbol).
    pub loc_file: String,
    /// Source location: start byte offset (0 for a bytecode symbol).
    pub loc_start: u32,
    /// Source location: end byte offset (0 for a bytecode symbol).
    pub loc_end: u32,
    /// Bytecode location: the container path (jar/jimage; empty for a source symbol).
    pub loc_container: String,
    /// Bytecode location: the binary class name (empty for a source symbol).
    pub loc_class: String,
    /// For a `Class`/`Interface`/`Enum`/`Record` symbol: an opaque, analyzer-owned
    /// JSON blob of the type's resolved member surface (supertypes + methods + fields),
    /// so a consumer resolves a project type's members straight from the index without
    /// re-parsing its source. Empty for non-type symbols. The index stays a leaf crate:
    /// it treats this as an opaque string — only the analyzer above (bennu-intel) knows
    /// its shape (a serialized `bennu_java::ClassMembers`).
    pub members_json: String,
}

/// One relation edge (docs §3). Stored in its own framed blob + fst (keyed by
/// `from_id`) so edge queries don't scan the symbol table.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct Relation {
    pub from_id: u32,
    pub to_id: u32,
    pub kind: RelationKind,
    pub source: Source,
}
