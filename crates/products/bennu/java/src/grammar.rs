//! The Java grammar itself, for callers that parse rather than ask.
//!
//! Everything else in this crate answers a *question* about Java source — what it declares,
//! what a name resolves to, where a span is. A syntax-tree view asks nothing: it wants the
//! grammar and walks the parse for itself.
//!
//! It lives here rather than being a `tree-sitter-java` dependency of every such caller,
//! because the grammar's version is a workspace-wide fact. `tree-sitter` is a `links` native
//! library: exactly one version can be linked, and the ABI shim `tree-sitter-java` is built
//! against must match it. A second crate picking its own is not a different choice, it is a
//! build failure — so the pin lives in one Cargo.toml and the rest ask for it here.

use tree_sitter::Language;

/// The Java grammar this workspace is built against.
///
/// Cheap: `LANGUAGE` is a static, and `into()` wraps a pointer. Callers that parse in a loop
/// should still hold a `Parser` rather than re-setting the language per file — that part is
/// not free.
pub fn language() -> Language {
    tree_sitter_java::LANGUAGE.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one thing worth asserting: a `Parser` accepts it. A grammar built against a
    /// different ABI than the runtime fails exactly here, and nowhere earlier.
    #[test]
    fn the_grammar_is_one_this_runtime_can_use() {
        let mut parser = tree_sitter::Parser::new();
        assert!(parser.set_language(&language()).is_ok());
        let tree = parser.parse("class A {}", None).expect("a parse");
        assert_eq!(tree.root_node().kind(), "program");
        assert!(!tree.root_node().has_error());
    }
}
