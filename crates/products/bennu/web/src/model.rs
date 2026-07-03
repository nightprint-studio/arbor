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

/// A parsed Struts `<interceptor name= class=>` def → a [`Source::StrutsInterceptor`]
/// symbol keyed by its name. The `class` FQCN resolves to the real Java type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterceptorRecord {
    pub name: String,
    /// The impl class FQCN. Empty for a built-in whose class is framework-provided.
    pub class: String,
    pub source_file: String,
    /// Byte offset of the `name` attribute value in `source_file` (go-to target).
    pub name_offset: usize,
}

/// A parsed Struts `<interceptor-stack name=>` def → a [`Source::StrutsInterceptor`]
/// symbol keyed by its name. `refs` is the ordered list of interceptor/stack names it
/// composes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterceptorStackRecord {
    pub name: String,
    pub refs: Vec<String>,
    pub source_file: String,
    /// Byte offset of the `name` attribute value in `source_file` (go-to target).
    pub name_offset: usize,
}

/// One `<interceptor-ref name=>` **use** — inside a stack (referrer = stack name), inside
/// an `<action>` (referrer = action qualified name), or a package
/// `<default-interceptor-ref>` (referrer empty, `is_default`). Powers find-usages of an
/// interceptor and the "ref names an unknown interceptor" diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterceptorRefUse {
    /// Stack name / action qualified-name that carries the ref; empty for a package default.
    pub referrer: String,
    /// The referenced interceptor / stack name.
    pub ref_name: String,
    /// True for a package `<default-interceptor-ref>` (no concrete referrer symbol).
    pub is_default: bool,
    pub source_file: String,
    /// Byte offset of the ref's `name` attribute value in `source_file` (the use site).
    pub name_offset: usize,
}

/// A parsed `<Action>-validation.xml` ruleset → a [`Source::StrutsValidation`] symbol
/// keyed by the validated action's class simple-name. Each `<field>` names an action
/// property (resolved to a getter/setter by the Java index in `bennu-intel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationRecord {
    /// The action class simple-name derived from the file name (`FooAction`).
    pub action_class: String,
    /// The action alias suffix if the file is `<Class>-<alias>-validation.xml`, else empty.
    pub alias: String,
    pub fields: Vec<ValidationField>,
    pub source_file: String,
}

/// One `<field name=>` in a validation ruleset + the validator types applied to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationField {
    /// The property name (`username`) — resolves to `getUsername`/`setUsername`.
    pub name: String,
    /// The `<field-validator type=>` types applied (e.g. `requiredstring`, `email`).
    pub validators: Vec<String>,
    /// Byte offset of the `name` attribute value in the file (go-to / precise diagnostic).
    pub name_offset: usize,
}

/// The kind of a MyBatis mapper statement element (`<select|insert|update|delete>`).
/// Mirrors [`RelKind`] living in this module: a small enum with a stable `as_str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Select,
    Insert,
    Update,
    Delete,
}

impl StatementKind {
    /// The lowercase element name (`select`/`insert`/`update`/`delete`) — for signatures
    /// and diagnostics.
    pub fn as_str(&self) -> &'static str {
        match self {
            StatementKind::Select => "select",
            StatementKind::Insert => "insert",
            StatementKind::Update => "update",
            StatementKind::Delete => "delete",
        }
    }
}

/// A parsed MyBatis `<mapper namespace="com.x.FooMapper">` element — a package-scoped
/// record set keyed by the mapper interface FQCN. Resolved graph-only (no fst symbol), the
/// same "config-as-name" model the interceptors use (docs precedent §0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapperRecord {
    /// The mapper interface FQCN: `com.x.FooMapper` (the join key to the Java type).
    pub namespace: String,
    /// The mapper `.xml` this was parsed from.
    pub source_file: String,
    /// Byte offset of the `namespace` attribute value in `source_file` (go-to the mapper).
    pub namespace_offset: usize,
}

/// A parsed MyBatis `<select|insert|update|delete id="bar">` statement — a name-keyed
/// record scoped to its owning `<mapper namespace>`. The Java→XML link is
/// `interface FQCN + method name → statement id`; resolved by name over the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRecord {
    /// Owning `<mapper namespace=>` FQCN — the join key (`com.x.FooMapper`).
    pub mapper_namespace: String,
    /// The `id` attribute value — the method name it maps (`findById`).
    pub id: String,
    /// Which statement element declared it (`Select`/`Insert`/`Update`/`Delete`).
    pub kind: StatementKind,
    /// Byte offset of the `id` attribute value start in the mapper file (go-to target).
    pub start: usize,
    /// Byte offset of the `id` attribute value end (exclusive) — a real span for the FE
    /// to select, a superset of the interceptor `name_offset` (start-only) pattern.
    pub end: usize,
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

/// The kind of form control an input field is — the wire label the FE shows next to a
/// field so it can distinguish a hidden id from a free-text box or a file upload. Mapped
/// from the field tag's local-name (+ an `<input type=>` for plain HTML).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormControl {
    /// A free-text input (`<input>` default, Struts `<s:textfield>`, `<html:text>`).
    Text,
    /// A masked password field (`<input type="password">`, `<s:password>`).
    Password,
    /// A hidden field (`<input type="hidden">`, `<s:hidden>`) — carries state, not user text.
    Hidden,
    /// A checkbox / checkbox-list (`<input type="checkbox">`, `<s:checkbox>`).
    Checkbox,
    /// A radio button (`<input type="radio">`, `<s:radio>`).
    Radio,
    /// A `<select>` / combobox / picker.
    Select,
    /// A multi-line `<textarea>`.
    TextArea,
    /// A submit control (`<input type="submit">`, `<s:submit>`).
    Submit,
    /// A file-upload control (`<input type="file">`, `<s:file>`).
    File,
    /// Any other recognized control we don't classify further.
    Other,
}

impl FormControl {
    /// The stable wire label (`text`/`password`/…) — for the FE + diagnostics.
    pub fn as_str(&self) -> &'static str {
        match self {
            FormControl::Text => "text",
            FormControl::Password => "password",
            FormControl::Hidden => "hidden",
            FormControl::Checkbox => "checkbox",
            FormControl::Radio => "radio",
            FormControl::Select => "select",
            FormControl::TextArea => "textarea",
            FormControl::Submit => "submit",
            FormControl::File => "file",
            FormControl::Other => "other",
        }
    }
}

/// One input field found inside a JSP `<form>` — an HTML `<input|textarea|select>` or a
/// Struts control (`<s:textfield>`, `<s:select>`, …). The byte span points at the field
/// **name value** inside the quotes (for a precise editor squiggle / caret round-trip),
/// mirroring the [`crate::jsp::JspActionRef`] span convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspFormField {
    /// The raw form-field name — the `name=` attribute (HTML + Struts), or the legacy
    /// `property=` for `<html:*>` controls. This is the key correlated against the action
    /// class's writable properties (setters) and its validation rules.
    pub name: String,
    /// What kind of control declared the field.
    pub control: FormControl,
    /// Start byte offset of the name value inside the quotes.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// One JSP `<form>` (HTML `<form>`, Struts `<s:form>`, or legacy `<html:form>`) + its input
/// fields. The correlation seam: the FE joins `action` → the resolved action class, then
/// matches each `fields[*].name` against that class's writable properties + validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspForm {
    /// The NORMALIZED action key (via [`crate::jsp::normalize_action_ref`] on the form's
    /// `action=`). `None` when the form has no `action=` or it is a computed expression.
    pub action: Option<String>,
    /// The form's `method=` (raw-lowercased: `get`/`post`), if present.
    pub method: Option<String>,
    /// Start byte offset of the `<form>` open tag.
    pub start: usize,
    /// End byte offset (exclusive): past the matching `</...form>` close, or the open tag's
    /// `>` when the close is missing (tolerant).
    pub end: usize,
    /// The input fields collected between the open tag and its matching close.
    pub fields: Vec<JspFormField>,
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
    /// `<interceptor-ref name>` → the interceptor/stack def it names.
    InterceptorRefToDef,
    /// `<interceptor name class>` → its impl FQCN.
    InterceptorToClass,
    /// mapper interface method (`<FQCN>#<method>`) → the `<select|...>` statement with the
    /// matching `id`. Graph-resolved by name (no fst symbol), so never resolves to ids at
    /// ingest — like the interceptor edges. MyBatis has no wildcards → always exact.
    MethodToStatement,
}

impl RelKind {
    /// Map onto the index' canonical [`RelationKind`] at ingestion.
    pub fn into_index(self) -> RelationKind {
        match self {
            RelKind::ActionToClass => RelationKind::ActionToClass,
            RelKind::ActionToResult => RelationKind::ActionToResult,
            RelKind::ResultToView => RelationKind::ResultToView,
            RelKind::BeanIdToImpl => RelationKind::BeanIdToImpl,
            RelKind::InterceptorRefToDef => RelationKind::InterceptorRefToDef,
            RelKind::InterceptorToClass => RelationKind::InterceptorToClass,
            RelKind::MethodToStatement => RelationKind::MethodToStatement,
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
    pub interceptors: Vec<InterceptorRecord>,
    pub interceptor_stacks: Vec<InterceptorStackRecord>,
    pub interceptor_refs: Vec<InterceptorRefUse>,
    pub validations: Vec<ValidationRecord>,
    pub mappers: Vec<MapperRecord>,
    pub statements: Vec<StatementRecord>,
    pub relations: Vec<Relation>,
}
