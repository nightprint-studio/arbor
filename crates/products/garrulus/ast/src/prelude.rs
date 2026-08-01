//! Canonical entry point for `garrulus-ast`'s public API.
//!
//! Workspace convention: call sites (`garrulus-parse`, `garrulus-vault`,
//! `garrulus-index`, `garrulus-be`) reach this crate through
//! `garrulus_ast::prelude::...`, never through the submodules. The submodules stay
//! `pub` for rustdoc navigation; the diff always goes through here.
//!
//! Two names would collide on a glob import and are therefore *not* re-exported
//! flat: [`crate::inline::plain_text`] (call it `inline::plain_text`, since
//! "plain text of what" needs the module to read) and the [`crate::walk`]
//! helpers, which are re-exported through the [`walk`] alias below so a consumer
//! writes `walk::visit_document(...)` and the traversal names keep their subject.

pub use crate::block::{Block, CalloutKind, ListItem, TaskState};
pub use crate::document::Document;
pub use crate::frontmatter::{FrontValue, Frontmatter};
pub use crate::inline::Inline;
pub use crate::io::{ReadError, Reader, WriteError, Writer};
pub use crate::span::Span;

/// The traversal helpers, kept behind their module name so `visit_blocks` reads
/// as `walk::visit_blocks` at the call site.
pub use crate::walk;

/// The inline module itself, for [`crate::inline::plain_text`].
pub use crate::inline;
