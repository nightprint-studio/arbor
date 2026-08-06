//! Canonical entry point for `bennu-xml`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_xml::prelude::...`. In practice a host needs [`XmlExtension`] and nothing else;
//! everything below it is reached through the [`FrameworkExtension`] trait.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

// The extension itself — what a host registers.
pub use crate::ext::XmlExtension;

// The schema sources and the rule that picks one.
pub use crate::catalog::{Catalog, SchemaFile};

// One model behind both grammar languages, plus the adapters.
pub use crate::grammar::{from_dtd, from_xsd, Attribute, Decl, Element, Grammar, GrammarKind};

// The built-in grammars — the two whose real schema is unreachable.
pub use crate::builtin::{
    grammar_for as builtin_grammar_for, pom as pom_grammar, taglib as taglib_grammar,
};

// Where the caret is, in XML's terms.
pub use crate::caret::{classify, Caret};

// The editor's answers.
pub use crate::intel::{completions, diagnostics, hover, inline_hint, navigate};

// The tolerant scan.
pub use crate::scan::{local_name, scan, Attr, Doctype, Scan, Tag, TagKind};
