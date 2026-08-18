//! Canonical entry point for `bennu-fulcrum-i18n`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_fulcrum_i18n::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// The extension a host registers — usually the only thing a caller needs.
pub use crate::ext::{is_rich, FulcrumI18nExtension};

// What an `i18n/` tree declares.
pub use crate::catalog::{Declaration, GlossaryDecl, LabelCatalog, Language, StyleDecl};

// What a translation is written in.
pub use crate::markup::{
    flatten, glossary_refs, parse_markup, placeholders, style_refs, MarkupProblem, Name, Parsed,
    Segment, SegmentKind,
};

// Where the project reads a label.
pub use crate::refs::{label_at, label_prefix_at, labels_in, looks_like_label, supports, LabelRef};
