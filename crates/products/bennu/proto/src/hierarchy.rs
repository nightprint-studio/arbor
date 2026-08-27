//! The call / type hierarchy on the wire.
//!
//! Its own module rather than a corner of `lsp` because it is no longer a language-server concept:
//! a `.java` buffer is answered by Bennu's own engine over the reference index, and a `.rs` one by
//! rust-analyzer. The panel that draws them is one panel and the shape it draws is one shape, which
//! is exactly what a shared wire type is for — the frontend asks "who calls this" and does not have
//! to know which engine answered.

use serde::{Deserialize, Serialize};

/// One node of a call or type hierarchy.
///
/// The two share a shape because the question differs only in direction, and because the tree is
/// fetched a level at a time either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub name: String,
    /// A lowercase kind name (`method`, `class`, `interface`, `struct`, `trait`).
    pub kind: String,
    #[serde(default)]
    pub detail: Option<String>,
    /// Where the declaration is — the name token, so go-to lands on it. **Empty** for an item with
    /// no source to open: a supertype that lives in a dependency jar is worth naming in the tree
    /// and cannot be jumped to.
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
    /// The trimmed source line, for a preview.
    pub preview: String,
    /// The call sites inside this node that reach the item asked about; empty for a type hierarchy.
    /// What lets a caller row jump to the call rather than to the function's head.
    #[serde(default)]
    pub call_sites: Vec<HierarchyCallSite>,
    /// The engine's own handle on this item, opaque. Sent back **verbatim** to fetch this node's
    /// children — it is a handle, not a description, so re-deriving it from the fields above would
    /// ask about something the engine never offered.
    pub handle: serde_json::Value,
}

/// One call site inside a hierarchy node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyCallSite {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub preview: String,
}
