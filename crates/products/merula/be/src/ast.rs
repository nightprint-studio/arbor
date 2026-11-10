//! `ast` domain — the Tree-sitter syntax tree of a `.merula` buffer, as the shared
//! syntax-tree panel draws it.
//!
//! Thin on purpose: `arbor-syntax` knows no language, so this only supplies the
//! grammar. The text always comes from the editor buffer (never re-read from disk),
//! so the byte ranges returned index into exactly what the user is looking at.

use arbor_syntax::prelude::{
    node_path_at, outline, ByteRange, OutlineOptions, SyntaxTree,
};
use merula::prelude::language;
use merula_core::prelude::MerulaState;
use serde::Deserialize;

/// What the panel asks for. Every field optional.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRequest {
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub max_nodes: Option<usize>,
    /// Hide anonymous nodes (punctuation, keywords). Off by default — a stray
    /// comma is very often the reason the parser read something differently.
    #[serde(default)]
    pub named_only: bool,
}

impl TreeRequest {
    fn options(&self) -> OutlineOptions {
        let defaults = OutlineOptions::default();
        OutlineOptions {
            max_depth: self.max_depth,
            max_nodes: self.max_nodes.or(defaults.max_nodes),
            named_only: self.named_only,
            text_preview: defaults.text_preview,
        }
    }
}

/// The syntax tree of a buffer.
#[arbor_rpc::handler]
fn merula_syntax_tree_of(
    _ctx: &MerulaState,
    text: String,
    request: Option<TreeRequest>,
) -> Result<SyntaxTree, String> {
    outline(&language(), &text, &request.unwrap_or_default().options())
        .map_err(|e| e.to_string())
}

/// The root-to-leaf chain of node ranges holding a byte offset ("reveal what the
/// cursor is in"). Ranges rather than ids: the panel keys its tree by range.
#[arbor_rpc::handler]
fn merula_syntax_path_at(
    _ctx: &MerulaState,
    text: String,
    offset: usize,
) -> Result<Vec<ByteRange>, String> {
    node_path_at(&language(), &text, offset).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "cps(0.5)\n\ntracks(\n  track(\"lead\", n(c4 e4 g4)),\n)\n";

    #[test]
    fn the_tree_covers_the_buffer() {
        let tree = outline(&language(), SRC, &TreeRequest::default().options()).expect("outlines");
        assert!(!tree.has_errors, "fixture must parse");
        assert_eq!(tree.root.range.start, 0);
    }

    #[test]
    fn the_path_to_a_name_ends_on_it() {
        let at = SRC.find("lead").expect("present");
        let path = node_path_at(&language(), SRC, at + 1).expect("walks");
        assert!(!path.is_empty());
    }
}
