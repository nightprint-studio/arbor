//! Canonical entry point for `bennu-jpa`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jpa::prelude::...`. In practice a host needs [`JpaExtension`] and the generators;
//! everything else is reached through the [`FrameworkExtension`] trait.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

// The extension itself — what a host registers.
pub use crate::ext::JpaExtension;

// The model a query answers from.
pub use crate::model::{
    capitalize, decapitalize, line_at, simple_name, strip_generics, type_argument, Entity,
    EntityField, JpaModel, MethodParam, QueryDef, RepoMethod, Repository,
};

// Derived query names: parsing, resolving, describing.
pub use crate::derived::{
    parse as parse_derived, resolve as resolve_derived, DerivedQuery, Issue as DerivedIssue,
    OrderTerm, Predicate, Subject,
};

// The query text inside a `@Query`.
pub use crate::hql::{from_entity, placeholders as query_placeholders, tokens as query_tokens, Placeholder, Token};

// Building the model.
pub use crate::index::{entities, repositories, JavaUnit};

// Generation — repositories, projections, query methods, and the entity-authoring half.
// Text only; nothing here writes to disk.
pub use crate::generate::{
    attribute_ddl, effective_name, entity_attribute, keyword_args, keyword_binds_collection, lifecycle_callback,
    method_name, modify_method, modify_method_name, named_query, named_query_name, projection,
    query_method, repository, AttributeSpec, Condition, Generated, GeneratedFile, Insertion,
    ModifySpec, NamedQuerySpec, QuerySpec, ReturnShape, CASCADE_TYPES,
    KEYWORDS as QUERY_KEYWORDS, LIFECYCLE_EVENTS, SUBJECTS as QUERY_SUBJECTS, VALIDATIONS,
};

// What a buffer is, and the toolbar that follows from it.
pub use crate::roles::{actions as file_actions, role_of, FileRole};

// JPA's annotation catalogue, for a consumer that needs the same origin check.
pub use crate::known::{
    find as find_annotation, has as has_annotation, is as is_annotation, is_any as is_any_annotation,
    RELATIONS,
};

// The scan + JPA's relevance markers.
pub use crate::scan::{looks_jpa_relevant, scan_java, JPA_MARKERS};
