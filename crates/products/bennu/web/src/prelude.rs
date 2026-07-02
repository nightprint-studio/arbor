//! Canonical entry point for `bennu-web`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_web::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// The emitted records + relations (the ingestion seam onto `bennu-index`).
pub use crate::model::{
    action_source, bean_source, ActionRecord, BeanRecord, RelKind, Relation, ResultRecord,
    TilesDefRecord, WebConfigGraph,
};

// The graph builder + the load-bearing resolution chains (docs §10 C1).
pub use crate::graph::{
    build as build_web_graph, relations_of, resolve_action_class, resolve_action_view, BuildReport,
    WebInputs,
};

// Struts wildcard support (candidate matching / backref expansion — docs §7).
pub use crate::struts::{join_ns, WildcardPattern};

// Spring bean-id → FQCN map (the C1 join) + the `<bean class="FQCN">` value spans a
// class-rename edits (docs §5 #10).
pub use crate::spring::{bean_class_value_spans, resolve_map as resolve_bean_map, BeanClassSpan};

// Tiles definition → JSP resolution.
pub use crate::tiles::{index as index_tiles, resolve_view as resolve_tiles_view};
