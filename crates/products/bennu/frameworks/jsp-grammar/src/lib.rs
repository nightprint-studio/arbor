//! `bennu-jsp-grammar` — the JSP Tree-sitter grammar, linked natively.
//!
//! ## Why the backend needs it too
//!
//! The grammar was written for the editor, and for a while the frontend was its only
//! consumer: the wasm build colours a page, and nothing on the Rust side ever had to read
//! one, because the JSP answers Bennu gave were all reachable with a tolerant tag scan
//! (`bennu-xml`).
//!
//! That stopped being true the moment two features needed a **tree**: the syntax-tree panel
//! (whose whole purpose is to show the parse — a second, approximate one would be a lie) and
//! structural search (which compares nodes, so a pattern and a page must be read by the same
//! grammar or nothing matches).
//!
//! So: one `grammar.js`, one generated `src/parser.c`, two builds of it. The wasm the editor
//! loads and the C this crate compiles come from the same generate, which is what makes "the
//! panel shows what the highlighter saw" a fact rather than a hope.
//!
//! ## What is deliberately not here
//!
//! Any reading of the tree. This crate is the grammar and the entry point to it; what a JSP
//! *means* — which library a tag belongs to, what an expression resolves to — is
//! `bennu-jsp`'s, and the tolerant scan stays where it is for the many answers that do not
//! want a parse at all.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jsp_grammar::prelude::...`.

pub mod prelude;

use tree_sitter::{Language, Parser, Tree};

extern "C" {
    fn tree_sitter_jsp() -> *const ();
}

/// The compiled JSP grammar.
///
/// Cheap to call — Tree-sitter hands back a pointer to the static parse tables — so a caller
/// needs no cache of its own.
pub fn jsp_language() -> Language {
    unsafe { Language::from_raw(tree_sitter_jsp().cast()) }
}

/// Parse a page.
///
/// `None` only if the runtime refuses the grammar, which a matching `tree-sitter` version
/// makes impossible — but a parser is the wrong place to panic, so the impossible case is a
/// value. Note that a *broken* page is not this case: the grammar is deliberately forgiving
/// (a stray `<` falls back to `text`), so it always produces a tree.
pub fn parse_jsp(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&jsp_language()).ok()?;
    parser.parse(source, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grammar loads against the runtime this workspace pins. A version skew between the
    /// generated tables and `tree-sitter` shows up here and nowhere else — every other test in
    /// the workspace would just see an empty tree.
    #[test]
    fn the_grammar_loads() {
        let mut parser = Parser::new();
        parser.set_language(&jsp_language()).expect("the runtime accepts the JSP tables");
    }

    #[test]
    fn a_page_parses_to_a_document() {
        let tree = parse_jsp("<html><body>hi</body></html>").expect("a tree");
        assert_eq!(tree.root_node().kind(), "document");
    }

    /// The reason the grammar exists: a namespaced closing tag is a closing tag, not an error.
    #[test]
    fn a_namespaced_tag_keeps_its_prefix_on_both_ends() {
        let source = "<s:iterator value=\"%{rows}\"><s:property value=\"%{code}\"/></s:iterator>";
        let tree = parse_jsp(source).expect("a tree");
        let mut names = Vec::new();
        let mut cursor = tree.walk();
        let mut todo = vec![tree.root_node()];
        while let Some(node) = todo.pop() {
            if node.kind() == "tag_name" {
                names.push(&source[node.start_byte()..node.end_byte()]);
            }
            todo.extend(node.children(&mut cursor));
        }
        assert!(names.contains(&"s:iterator"), "got {names:?}");
        assert!(names.contains(&"s:property"), "got {names:?}");
    }

    /// Forgiving by construction — the highlighter cannot afford an ERROR node that swallows
    /// the rest of the file, and neither can the panel that draws the same tree.
    #[test]
    fn a_stray_angle_bracket_does_not_poison_the_parse() {
        let tree = parse_jsp("a < b && c > d").expect("a tree");
        assert_eq!(tree.root_node().kind(), "document");
    }

    /// An expression is a **tree**, not a token: a path is a subtree and everything else is a
    /// sibling. What lets a structural search say `%{#session.$prop$}` and a resolver stop
    /// re-scanning the text by hand.
    #[test]
    fn an_expression_is_decomposed_into_its_parts() {
        let tree = parse_jsp("%{#session.currentUser != null}").expect("a tree");
        assert_eq!(
            kinds(&tree),
            [
                "document",
                "ognl_expression",
                "el_path",
                "el_context",
                "el_identifier",
                "el_property",
                "el_operator",
                "el_path",
                "el_identifier",
            ]
        );
    }

    /// The property this decomposition was not allowed to cost: an unterminated `${` is what
    /// the file looks like between two keystrokes, and it must not swallow the page. The error
    /// stops at the next tag, and the markup after it parses as markup.
    #[test]
    fn an_unterminated_expression_does_not_swallow_the_page() {
        let source = "<p>a ${user.</p>\n<div>after</div>";
        let tree = parse_jsp(source).expect("a tree");
        let named = kinds(&tree);
        assert!(named.contains(&"end_tag".to_string()), "the close tag survived: {named:?}");
        assert_eq!(named.iter().filter(|k| *k == "start_tag").count(), 2);
        // Whatever the parser made of the fragment, it is confined to it.
        let error = find_error(tree.root_node()).expect("an error node on the fragment");
        assert!(error.end_byte() <= source.find("</p>").expect("the close"));
    }

    /// Every named node kind, in order — a flat reading of the tree that a test can assert on.
    fn kinds(tree: &tree_sitter::Tree) -> Vec<String> {
        let mut out = Vec::new();
        let mut cursor = tree.walk();
        let mut todo = vec![tree.root_node()];
        while let Some(node) = todo.pop() {
            if node.is_named() {
                out.push(node.kind().to_string());
            }
            let mut children: Vec<_> = node.children(&mut cursor).collect();
            children.reverse();
            todo.extend(children);
        }
        out
    }

    fn find_error(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
        if node.is_error() {
            return Some(node);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children.into_iter().find_map(find_error)
    }
}
