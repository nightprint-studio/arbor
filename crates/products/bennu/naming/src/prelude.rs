//! Canonical entry point for `bennu-naming`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_naming::prelude::...`.

pub use crate::config::{LanguageRules, NamingConfig, NamingOverride};
pub use crate::convention::Convention;
pub use crate::pack::{pack_for_path, packs, DeclSource, Declared, GrammarWalk, Pack};
pub use crate::scan::{
    diagnostics, diagnostics_from_symbols, needs_symbols, violation_at, violation_at_from_symbols,
    violations, violations_from_symbols, Violation,
};
pub use crate::skip::{is_generated, is_generated_path, is_generated_source};
pub use crate::target::Target;
pub use crate::words::{tokenize_identifier, SubWord};

// The wire diagnostic the scan emits, re-exported so a consumer reaches it through this prelude.
pub use bennu_proto::prelude::Diagnostic;
