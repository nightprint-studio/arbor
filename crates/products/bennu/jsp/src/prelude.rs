//! Canonical entry point for `bennu-jsp`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jsp::prelude::...`. In practice a host needs [`JspExtension`] and nothing else;
//! everything below it is reached through the [`FrameworkExtension`] trait.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

// The extension itself — what a host registers.
pub use crate::ext::JspExtension;

// Which library a `uri="…"` means, and the `web.xml` aliases that can rename one.
pub use crate::catalog::{web_xml_aliases, TaglibCatalog};

// The page's own declarations.
pub use crate::directives::{taglib_directives, TaglibDirective};

// The tag-library model + its parser.
pub use crate::tld::{parse_tld, AttrDecl, FunctionDecl, TagDecl, Taglib};

// The editor's answers.
pub use crate::intel::{completions, diagnostics, hover, navigate};
