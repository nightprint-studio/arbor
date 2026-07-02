//! The records + relations the web-config parsers emit — the ingestion seam.
//!
//! These are **string-keyed** (action qualified name, Spring bean-id, FQCN, Tiles def
//! name, JSP path): the config graph is resolved by *name*, and the integration
//! (`bennu-intel` / `bennu-be`) turns each record into a [`bennu_index`] [`Symbol`] and
//! each edge into a [`Relation`] with resolved `u32` ids as it ingests. This crate never
//! allocates ids — it owns the *shape* of the graph, not the index.
//!
//! [`Symbol`]: bennu_index::prelude::Symbol
//! [`Relation`]: bennu_index::prelude::Relation

use bennu_index::prelude::{RelationKind, Source};

/// A parsed Struts/XWork `<action>` mapping → a [`Source::StrutsAction`] symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRecord {
    /// Fully-qualified action name: `<namespace>/<name>` (e.g. `/do/Category/viewTree`).
    /// This is the key a JSP form/link resolves against.
    pub qualified_name: String,
    /// The package `namespace` (e.g. `/do/Category`), empty if none.
    pub namespace: String,
    /// The raw `name` attribute (may contain wildcards, e.g. `editAttribute*`).
    pub name: String,
    /// The `class` attribute — in this codebase a **Spring bean-id**, not an FQCN
    /// (docs §10 C1). Empty if omitted (defaults to the framework `ActionSupport`).
    pub class_ref: String,
    /// The `method` attribute (may contain `{1}` backrefs when the action is a
    /// wildcard). Empty → `execute`.
    pub method: String,
    /// Whether the action `name` contains a `*` wildcard → nav is to *candidates*,
    /// marked inferred (docs §7).
    pub is_wildcard: bool,
    /// Config fragment this action was parsed from.
    pub source_file: String,
}

/// A parsed Struts `<result>` inside an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRecord {
    /// Owning action's qualified name.
    pub action_qualified_name: String,
    /// The result `name` (empty attr defaults to `success`).
    pub name: String,
    /// The result `type` (`tiles`, `dispatcher`, `chain`, `redirectAction`, …).
    pub result_type: String,
    /// The result body / target: for `type="tiles"` a Tiles definition name; for
    /// `dispatcher` a JSP path; for `chain`/`redirectAction` an action name.
    pub target: String,
    /// True when the target/method is computed at runtime (`{1}` backref) or the owning
    /// action is a wildcard — a *candidate* edge, never an exact "missing" verdict.
    pub is_inferred: bool,
}

/// A parsed Spring `<bean>` → a [`Source::SpringBean`] symbol. The id→FQCN join that
/// turns `<action class="beanId">` into a real class (docs §10 C1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeanRecord {
    pub id: String,
    /// The `class` attribute FQCN. Empty when the bean only `parent=`s another (no own
    /// class) — still recorded so the id resolves via its parent chain.
    pub class: String,
    /// The `parent` bean id, if any (the abstract-parent pattern is pervasive here).
    pub parent: String,
    pub source_file: String,
}

/// A parsed Tiles `<definition>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TilesDefRecord {
    pub name: String,
    /// The `template` attribute JSP, if the definition declares one directly.
    pub template: String,
    /// The `extends` parent definition name, if any (the `extends="main.layout"`
    /// pattern — the per-action view then lives in the `body` put-attribute).
    pub extends: String,
    /// The `<put-attribute name="body" value="…jsp">` JSP — the meaningful per-action
    /// view target in this codebase (96/97 defs carry the view here, not in `template`).
    pub body_jsp: String,
    pub source_file: String,
}

impl TilesDefRecord {
    /// The best JSP this definition resolves to for "go to view": the direct `template=`
    /// if present, else the `body` put-attribute JSP. May be empty when the view is only
    /// inherited from `extends` (resolve via the parent then — see [`crate::tiles`]).
    pub fn view_jsp(&self) -> &str {
        if !self.template.is_empty() {
            &self.template
        } else {
            &self.body_jsp
        }
    }
}

/// Relation edge kind for the config graph. A subset of the index'
/// [`RelationKind`], plus [`Self::into_index`] to map onto it at ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelKind {
    /// action → its `class` (a Spring bean-id here).
    ActionToClass,
    /// action → one of its `<result>`s.
    ActionToResult,
    /// result → the view it renders (Tiles def name or JSP path).
    ResultToView,
    /// bean-id → its impl FQCN.
    BeanIdToImpl,
}

impl RelKind {
    /// Map onto the index' canonical [`RelationKind`] at ingestion.
    pub fn into_index(self) -> RelationKind {
        match self {
            RelKind::ActionToClass => RelationKind::ActionToClass,
            RelKind::ActionToResult => RelationKind::ActionToResult,
            RelKind::ResultToView => RelationKind::ResultToView,
            RelKind::BeanIdToImpl => RelationKind::BeanIdToImpl,
        }
    }
}

/// One emitted edge. `from`/`to` are string keys (action qualified name, bean-id, class
/// FQCN, Tiles def name, JSP path); the integration resolves them to `Symbol.id`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub from: String,
    pub to: String,
    pub kind: RelKind,
    /// True for candidate edges (wildcard action, `{1}` backref, Tiles indirection) —
    /// never emit an exact "missing" verdict on these (docs §8).
    pub inferred: bool,
}

/// The [`Source`] tag a record of each kind carries once ingested.
pub fn action_source() -> Source {
    Source::StrutsAction
}
/// The [`Source`] tag a Spring bean record carries once ingested.
pub fn bean_source() -> Source {
    Source::SpringBean
}

/// Everything the config-graph parse produces for a project. The integration ingests
/// the record vecs as [`Symbol`]s and the relations as [`Relation`] edges.
///
/// [`Symbol`]: bennu_index::prelude::Symbol
/// [`Relation`]: bennu_index::prelude::Relation
#[derive(Debug, Default, Clone)]
pub struct WebConfigGraph {
    pub actions: Vec<ActionRecord>,
    pub results: Vec<ResultRecord>,
    pub beans: Vec<BeanRecord>,
    pub tiles_defs: Vec<TilesDefRecord>,
    pub relations: Vec<Relation>,
}
