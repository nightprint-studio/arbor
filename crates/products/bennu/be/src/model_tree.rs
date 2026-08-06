//! `model` view — the **AST**: the same parse read in Java's vocabulary, all the way down.
//!
//! The syntax-tree panel's other tab, and the two answer different questions:
//!
//! * [`crate::ast`] shows the **parse**: every node the grammar built, punctuation included. It
//!   answers *why did it read my file that way*.
//! * this shows what Bennu **understood** — statements, expressions, and the type of each — which
//!   is what the index, completion, the checks and go-to all reason over. It answers *what does
//!   Bennu think this code does*.
//!
//! Which matters because the two can disagree, and when they do the second is the one that
//! explains a wrong answer.
//!
//! ## The lowering is not here
//!
//! It is [`bennu_java::prelude::lower_ast`], next to the parser and the inference it needs.
//! Everything in this file is the seam: pick the language, hand it the project's resolver, and
//! map the result onto the wire shape the panel already draws.
//!
//! ## The same shape as the parse tree, deliberately
//!
//! It answers in [`arbor_syntax::prelude::SyntaxTree`], so the panel's filtering, its expansion,
//! its click-to-select and its follow-the-caret are the ones already written. Three fields carry
//! the mapping:
//!
//! | Field | Holds |
//! |---|---|
//! | `kind` | what it **is** — `class`, `method`, `if`, `call`, `local variable` |
//! | `field` | the **role** it plays — `condition`, `receiver`, `argument`; or, on a declaration with no role, its modifiers |
//! | `text` | the **name**, and the **resolved type** after it |
//!
//! The role and the modifiers share a column because they are the same thing to a reader: what
//! makes this row different from the one under it.

use arbor_syntax::prelude::{ByteRange, SyntaxNode, SyntaxTree};
use bennu_core::prelude::BennuState;
use bennu_java::prelude::AstNode;

use crate::ast::{AstAnswer, SyntaxArgs};
use crate::index_service::IndexService;

/// Build the model tree of `text`, read as whatever `path`'s extension says it is.
///
/// `None` for a language Bennu has no model of — the same honest refusal [`crate::ast`] makes,
/// and for the same reason: "Bennu does not model XML" is a statement about the tool, and a blank
/// panel would be an implied one about the file.
fn model_of(text: &str, path: &str) -> AstAnswer {
    let language = crate::ast::language_name_of(path).to_string();
    if !crate::ast::is_java(path) {
        return AstAnswer { language, tree: None };
    }
    let ast = IndexService::global().ast_of(path, text);
    AstAnswer { language, tree: Some(tree_of(&ast, text)) }
}

/// The AST as the panel's tree. Pure — takes the lowered nodes and the source they came from —
/// so every shape below is unit-testable without a backend.
fn tree_of(ast: &AstNode, source: &str) -> SyntaxTree {
    let root = node_of(ast, source);
    let node_count = count(&root);
    // Never truncated — an AST has as many rows as the file has constructs, and no walk budget
    // applies. `has_errors` stays false because a parse error is a fact about the parse, and the
    // parse has its own tab to report it on.
    SyntaxTree { root, node_count, truncated: false, has_errors: false }
}

fn node_of(ast: &AstNode, source: &str) -> SyntaxNode {
    SyntaxNode {
        kind: ast.kind.clone(),
        // The role when there is one, the modifiers otherwise: a `condition` is never also
        // `private final`, so the column never has to choose.
        field: ast.role.clone().or_else(|| ast.modifiers.clone()),
        named: true,
        range: ByteRange::new(ast.span.start.min(source.len()), ast.span.end.min(source.len())),
        line: line_of(source, ast.span.start),
        text: label_of(ast),
        synthesized: ast.synthesized,
        children: ast.children.iter().map(|c| node_of(c, source)).collect(),
        ..SyntaxNode::default()
    }
}

/// The row's text: what it is called, and what it resolved to.
///
/// `→` rather than `:` when the name denotes a **class**, because "this expression has type
/// `Files`" and "this name *is* the class `Files`" are different statements and the row that
/// conflated them would be the one hiding a static call inside an instance one.
fn label_of(ast: &AstNode) -> Option<String> {
    match (&ast.label, &ast.type_name) {
        (Some(label), Some(ty)) if ast.names_a_type => Some(format!("{label} → {ty}")),
        (Some(label), Some(ty)) => Some(format!("{label} : {ty}")),
        (Some(label), None) => Some(label.clone()),
        (None, Some(ty)) => Some(format!(": {ty}")),
        (None, None) => None,
    }
}

/// 1-based line of a byte offset. Counted here rather than carried on the AST, because a line
/// number is a presentation of a byte offset and storing both is how they drift.
fn line_of(source: &str, at: usize) -> usize {
    source.get(..at).map(|head| head.matches('\n').count() + 1).unwrap_or(1)
}

fn count(node: &SyntaxNode) -> usize {
    1 + node.children.iter().map(count).sum::<usize>()
}

/// The AST of a buffer.
#[arbor_rpc::handler]
fn bennu_symbol_tree_of(_ctx: &BennuState, args: SyntaxArgs) -> Result<AstAnswer, String> {
    Ok(model_of(&args.text, &args.path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Untyped, which is what a unit test can build: the resolver is a project, and every shape
    /// this module is responsible for is independent of it.
    fn tree(source: &str) -> SyntaxNode {
        tree_of(&bennu_java::prelude::lower_ast(source, None), source).root
    }

    fn find<'t>(node: &'t SyntaxNode, kind: &str) -> Option<&'t SyntaxNode> {
        if node.kind == kind {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, kind))
    }

    const SOURCE: &str = "package com.acme;\n\
                          public class OrderDao {\n\
                          \x20 private final Connection conn;\n\
                          \x20 public List<Order> findAll(int page) {\n\
                          \x20   if (page < 0) { log.debug(\"bad\"); }\n\
                          \x20   return null;\n\
                          \x20 }\n\
                          }";

    /// **The whole point.** The declaration model stopped at the opening brace; this does not.
    #[test]
    fn a_method_body_reaches_the_panel() {
        let root = tree(SOURCE);
        let branch = find(&root, "if").expect("the if");
        let call = find(branch, "call").expect("the call inside it");
        assert_eq!(call.text.as_deref(), Some("debug"));
    }

    /// The role and the modifiers share the accent column, and a row never needs both.
    #[test]
    fn the_accent_column_carries_the_role_or_the_modifiers() {
        let root = tree(SOURCE);
        assert_eq!(find(&root, "class").and_then(|n| n.field.as_deref()), Some("public"));
        assert_eq!(find(&root, "field").and_then(|n| n.field.as_deref()), Some("private final"));
        let branch = find(&root, "if").expect("the if");
        assert_eq!(branch.children[0].field.as_deref(), Some("condition"));
    }

    /// Every row must select its own bytes — the reason the whole tree carries spans.
    #[test]
    fn a_rows_range_covers_what_it_names() {
        let root = tree(SOURCE);
        let call = find(&root, "call").expect("the call");
        assert_eq!(&SOURCE[call.range.start..call.range.end], "log.debug(\"bad\")");
        let literal = find(call, "string").expect("the literal");
        assert_eq!(&SOURCE[literal.range.start..literal.range.end], "\"bad\"");
    }

    /// A resolved type belongs to the row, after the name. Untyped here — the assertion is that
    /// the absence reads as an absence rather than as an empty `:`.
    #[test]
    fn a_row_with_no_resolved_type_shows_only_its_name() {
        let root = tree(SOURCE);
        let call = find(&root, "call").expect("the call");
        assert_eq!(call.text.as_deref(), Some("debug"), "no dangling separator");
    }

    /// The refusal has to be a statement about Bennu, not about the file — the panel says "no
    /// model for XML", and an empty tree would say "this file is empty".
    #[test]
    fn a_language_with_no_model_is_named_rather_than_drawn_empty() {
        let answer = model_of("<beans/>", "/p/beans.xml");
        assert_eq!(answer.language, "XML");
        assert!(answer.tree.is_none());
    }

    #[test]
    fn the_node_count_is_the_rows_there_are() {
        let root = tree("class A { int x; }");
        // the unit, the class, the field — and no package row, because there is no package.
        assert_eq!(count(&root), 3);
    }
}
