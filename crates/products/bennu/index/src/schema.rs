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
    /// A Struts `<interceptor>` or `<interceptor-stack>` definition (config-as-symbol),
    /// keyed by its name.
    StrutsInterceptor,
    /// A Struts validation ruleset (`<Action>-validation.xml`), keyed by the validated
    /// action's class simple-name.
    StrutsValidation,
    /// A MyBatis mapper `<select|insert|update|delete>` statement (config-as-symbol),
    /// keyed by `<interface FQCN>#<statement id>`. Resolved graph-only (no fst symbol,
    /// like [`Self::StrutsInterceptor`]) — the tag exists for source-parity + edge labels.
    MyBatisMapper,
}

/// The kind of a [`Relation`] edge (docs §3). Java type-hierarchy edges plus the
/// config-graph edges that make the XML a first-class language.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
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
    /// An `<interceptor-ref name="x">` → the `<interceptor>`/`<interceptor-stack>` def it
    /// names (the go-to / find-usages edge for interceptor wiring).
    InterceptorRefToDef,
    /// An `<interceptor name="x" class="FQCN">` → its impl class (like [`Self::BeanIdToImpl`],
    /// the FQCN lives on the interceptor symbol, `to_id` is `u32::MAX`).
    InterceptorToClass,
    /// A mapper interface method (`<FQCN>#<method>`) → the MyBatis `<select|...>` statement
    /// with the matching `id` (go-to XML ↔ find-usages Java). Graph-resolved by name like
    /// the interceptor edges; no global fst symbol, so it is dropped at ingest — both
    /// directions are graph queries (statement ids aren't globally-unique fst keys).
    MethodToStatement,
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
    /// A **candidate** edge (a Struts wildcard action, a `{1}` backref, Tiles
    /// indirection): navigation goes to candidates and a diagnostic must NEVER treat a
    /// candidate as an exact "missing" verdict (docs §7/§8). Concrete edges are `false`.
    pub inferred: bool,
}
