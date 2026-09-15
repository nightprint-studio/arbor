//! Canonical entry point for `bennu-toolconf`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_toolconf::prelude::...`. In practice a host needs exactly one thing — the extension to
//! register; the rest is here for a caller that wants the vocabulary without the editor around it.

// What a host registers.
pub use crate::ext::ToolConfExtension;

// The vocabulary, and what one entry in it is.
pub use crate::junit::CATALOGUE as JUNIT_CATALOGUE;
pub use crate::lombok::catalogue as lombok_catalogue;
pub use crate::model::{Catalogue, ConfigKey};

// The editor answers, for a caller with its own context rather than the extension's.
pub use crate::answers::{
    completions, diagnostics, hover, inline_hint, CODE_BAD_VALUE, CODE_DEPRECATED, CODE_TOO_NEW,
};

// The syntax, and which version of the tool a project is on.
pub use crate::props::{classify, entries, key_at, value_at, Caret, Entry};
pub use crate::version::resolve as resolve_tool_version;
