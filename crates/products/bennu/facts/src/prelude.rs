//! Canonical entry point for `bennu-facts`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_facts::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// The scan and everything it produces. `AnnFacts` carries the arguments with their spans,
// which is the whole reason a framework extension parses Java a second time.
pub use crate::scan::{
    mentions_any, scan_java, AnnFacts, AnnString, FieldFacts, JavaFacts, MethodFacts, ParamFacts,
    TypeFacts,
};

// Deciding whether an annotation is the framework's or the project's own. The table is the
// caller's; the resolution order is the compiler's.
pub use crate::origin::{resolves_to, AnnotationTable, KnownAnnotation};
