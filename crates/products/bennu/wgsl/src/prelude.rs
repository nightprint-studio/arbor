//! Canonical entry point for `bennu-wgsl`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_wgsl::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

pub use crate::builtins::{
    completions_for, Builtin, ATTRIBUTES, BUILTIN_FUNCTIONS, BUILTIN_TYPES, BUILTIN_VALUES,
    KEYWORDS,
};
pub use crate::imports::{
    context_at as import_context_at, defined_path, ImportContext, ImportPath, BEVY_IMPORTS,
};
pub use crate::symbols::{
    doc_above, signature_at,
    occurrences_of, scan as scan_symbols, symbol_at, WgslSymbol, WgslSymbolKind,
};
pub use crate::validate::{
    preprocessor_reason, validate, WgslDiagnostic, WgslReport, WgslSeverity,
};
