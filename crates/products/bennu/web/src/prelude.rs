//! Canonical entry point for `bennu-web`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_web::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// The emitted records + relations (the ingestion seam onto `bennu-index`).
pub use crate::model::{
    action_source, bean_source, ActionRecord, BeanRecord, InterceptorRecord, InterceptorRefUse,
    InterceptorStackRecord, MapperRecord, RelKind, Relation, ResultRecord, StatementKind,
    StatementRecord, TilesDefRecord, ValidationField, ValidationRecord, WebConfigGraph,
};

// The graph builder + the load-bearing resolution chains (docs §10 C1).
pub use crate::graph::{
    build as build_web_graph, interceptor_usages, methods_for_mapper, relations_of,
    resolve_action_class, resolve_action_view, resolve_interceptor_ref, statement_for_method,
    validations_for_class, BuildReport, InterceptorDef, StatementTarget, WebInputs,
};

// Struts interceptor + validation parsing (standalone entry points; the project build
// folds interceptors into the struts include-graph walk).
pub use crate::interceptors::{parse_file as parse_interceptors, InterceptorParse};
pub use crate::validation::{parse_file as parse_validation, split_validation_filename};

// JSP action-reference + taglib scan (feeds the unknown-action squiggle + find-usages).
pub use crate::jsp::{
    normalize_action_ref, parse_jsp, parse_jsp_file, JspActionRef, JspParse, JspTaglib,
};

// The form records live in `model` (like the other emitted records).
pub use crate::model::{FormControl, JspForm, JspFormField};

// JSP `<form>` scan (form → action → fields, for the form/field-binding inspector).
pub use crate::forms::{parse_jsp_forms, parse_jsp_forms_file};

// Struts wildcard support (candidate matching / backref expansion — docs §7).
pub use crate::struts::{join_ns, WildcardPattern};

// Spring bean-id → FQCN map (the C1 join) + the `<bean class="FQCN">` value spans a
// class-rename edits (docs §5 #10).
pub use crate::spring::{bean_class_value_spans, resolve_map as resolve_bean_map, BeanClassSpan};

// Tiles definition → JSP resolution.
pub use crate::tiles::{index as index_tiles, resolve_view as resolve_tiles_view};

// MyBatis mapper-XML parsing (mapper namespace + statement records; graph-only by name).
pub use crate::mybatis::{parse_mybatis, parse_mybatis_file, MyBatisParse};
