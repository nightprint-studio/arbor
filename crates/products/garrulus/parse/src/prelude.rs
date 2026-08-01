//! Canonical entry point for `garrulus-parse`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `garrulus_parse::prelude::...`, never through the submodules.

pub use crate::callout::{format_callout_header, parse_callout_header, CalloutHeader};
pub use crate::frontmatter::{build_frontmatter, parse_front_map, split_frontmatter};
pub use crate::grammar::block_language;
pub use crate::reader::{read_document, MarkdownReader};
pub use crate::scan::scan_inlines;
pub use crate::writer::{write_document, MarkdownWriter};

// The traits are re-exported because a caller holding a `MarkdownReader` needs
// `Reader` in scope to call `.read()`, and making it name a second crate for
// that would be a poor trade for purity.
pub use garrulus_ast::prelude::{Reader, Writer};
