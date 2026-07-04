//! Canonical entry point for `bennu-web`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_web::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// The emitted records + relations (the ingestion seam onto `bennu-index`).
pub use crate::model::{
    action_source, bean_source, ActionRecord, BeanRecord, FieldValidator, InterceptorRecord,
    InterceptorRefUse, InterceptorStackRecord, MapperRecord, RelKind, Relation, ResultRecord,
    StatementKind, StatementRecord, TilesDefRecord, ValidationField, ValidationRecord,
    ValidatorMessage, ValidatorParam, WebConfigGraph,
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
pub use crate::validation::{
    parse_file as parse_validation, split_validation_filename, validation_file_for_class,
};

// Pure authoring of `*-validation.xml` (create skeleton / a `<field>` chain / append) + the
// validator vocabulary registry — the write side of the validation feature.
pub use crate::validation_author::{
    append_validator, author_field_block, author_field_validator, author_validation_skeleton,
    AuthoredMessage, AuthoredValidator,
};
pub use crate::validator_catalog::{all_validators, validator_def, ParamDef, ParamKind, ValidatorDef};

// JSP action-reference + taglib scan (feeds the unknown-action squiggle + find-usages).
pub use crate::jsp::{
    normalize_action_ref, parse_jsp, parse_jsp_file, JspActionRef, JspParse, JspTaglib,
};

// JSP include / view-reference scan + path resolution (go-to on `<%@ include %>`,
// `<jsp:include>`, `<s:include>`, `<c:import>` → the referenced on-disk JSP).
pub use crate::jsp_includes::{
    parse_jsp_includes, parse_jsp_includes_file, resolve_include_target, unresolved_includes,
    unresolved_includes_file, JspInclude,
};

// JSP include GRAPH (project-wide forward+reverse include edges) + a cycle-safe transitive
// walk from a start file — powers the include-aware Forms tool window.
pub use crate::include_graph::{
    build_include_graph, related_files, IncludeGraph, IncludeRelation, RelatedFile, RelatedFiles,
};

// The form records live in `model` (like the other emitted records).
pub use crate::model::{FormControl, JspForm, JspFormField};

// JSP `<form>` scan (form → action → fields, for the form/field-binding inspector) + the
// all-fields scan a fragment (no enclosing `<form>`) contributes to its parent's form.
pub use crate::forms::{parse_jsp_fields, parse_jsp_fields_file, parse_jsp_forms, parse_jsp_forms_file};

// Include-aware form field aggregation: the complete parameter set a `<form>` posts once its
// `<jsp:include>`d fragments are spliced in (both directions across the include graph).
pub use crate::form_expand::{
    analyze_forms_expanded, ExpandedField, ExpandedForm, ExpandedForms,
};

// Incremental, persistable include-graph cache (avoids re-parsing every JSP per tab switch).
pub use crate::include_cache::{file_stamp, IncludeGraphCache};

// JSP page-scoped variable navigation (`<c:set>`/`<s:set>`/… declarations + `${var}`/`%{var}`
// references) — go-to-declaration + find-usages for JSP-local variables.
pub use crate::jsp_vars::{
    line_col, parse_jsp_vars, parse_jsp_vars_file, var_declaration, var_name_at, var_usages,
    JspVarDecl, JspVarRef, JspVars,
};

// Struts wildcard support (candidate matching / backref expansion — docs §7).
pub use crate::struts::{join_ns, WildcardPattern};

// Spring bean-id → FQCN map (the C1 join) + the `<bean class="FQCN">` value spans a
// class-rename edits (docs §5 #10).
pub use crate::spring::{bean_class_value_spans, resolve_map as resolve_bean_map, BeanClassSpan};

// Tiles definition → JSP resolution.
pub use crate::tiles::{index as index_tiles, resolve_view as resolve_tiles_view};

// MyBatis mapper-XML parsing (mapper namespace + statement records; graph-only by name).
pub use crate::mybatis::{parse_mybatis, parse_mybatis_file, MyBatisParse};

// MyBatis mapper-XML navigation: resolve the token under the caret (statement id → Java
// method, namespace → interface, include/resultMap → their fragment, intra- or cross-file).
pub use crate::mybatis_nav::{resolve_mybatis_ref, FragmentKind, MybatisRef};
