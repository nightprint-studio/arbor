//! **Inline variable** — put the value back where the name was, and delete the declaration.
//!
//! The inverse of *extract variable*, and the one refactoring people reach for on somebody else's
//! code: a local that is assigned once and used once is a name in the way of reading the line.
//!
//! ## The three things that make it unsafe, and are therefore refused
//!
//! - **Reassignment.** `int x = 1; … x = 2; … use(x)` — inlining the initialiser puts `1` where the
//!   program means `2`. Checked against every assignment and `++`/`--` in the method.
//! - **A capture that moves.** The initialiser reads a variable that is itself reassigned between
//!   the declaration and a use, so evaluating it later gives a different answer. This is the subtle
//!   one, and it is why this is a tree walk and not a search-and-replace.
//! - **A side effect used more than once.** `int n = next(); use(n); use(n);` becomes two calls.
//!   Refused whenever the initialiser is not a pure read and there is more than one use.
//!
//! ## Parentheses
//!
//! `int t = a + b; return t * 2;` must become `return (a + b) * 2;` and not `return a + b * 2;`.
//! Handled structurally: the initialiser is wrapped whenever it is a compound expression being
//! dropped into a position that binds tighter — which is decided by the *kind* of the parent node,
//! not by counting operators.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{
    descendants, enclosing, enclosing_callable, identifier_at, identifiers, is_expression, text,
};

const ID: (&str, &str) = ("inline-variable", "Inline variable");

/// Plan an *inline variable* for the local under the caret — either its declaration or one of its
/// uses.
pub fn inline_variable(root: Node<'_>, source: &str, offset: usize) -> Outcome {
    let (id, label) = ID;
    let name_node = identifier_at(root, offset)?;
    let name = text(&name_node, source);
    let method = enclosing_callable(name_node)?;

    // The declaration this name belongs to. A parameter has no initialiser to inline, and saying so
    // is more useful than the row not being there — it is the commonest wrong guess.
    let Some(declarator) = declarator_of(&method, name, source) else {
        if is_parameter(&method, name, source) {
            return Some(Err(Refusal::new(id, label, "a parameter has no value to inline here")));
        }
        return None;
    };
    let Some(value) = declarator.child_by_field_name("value") else {
        return Some(Err(Refusal::new(id, label, "this variable is declared without a value")));
    };
    let declaration = enclosing(declarator, &["local_variable_declaration"])?;

    // One declarator per statement, or deleting the statement takes the others with it.
    if descendants(declaration, "variable_declarator").len() > 1 {
        return Some(Err(Refusal::new(
            id,
            label,
            "this statement declares more than one variable — split it first",
        )));
    }

    // Every read of the name in this method. `is_declaration_name` is what excludes the declarator's
    // own name node, which is an `identifier` like any other and would otherwise be "inlined" into
    // itself.
    let uses: Vec<Node<'_>> = identifiers(method)
        .into_iter()
        .filter(|n| text(n, source) == name && !is_declaration_name(n))
        .collect();

    if let Some(reason) = unsafe_to_inline(&method, &declaration, &value, name, &uses, source) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    if uses.is_empty() {
        return Some(Err(Refusal::new(
            id,
            label,
            "nothing reads this variable — deleting it is a different fix",
        )));
    }

    let replacement = text(&value, source);
    let mut edits: Vec<RefactorEdit> = uses
        .iter()
        .map(|use_node| {
            let text = match needs_parentheses(&value, use_node) {
                true => format!("({replacement})"),
                false => replacement.to_string(),
            };
            RefactorEdit::new(use_node.start_byte(), use_node.end_byte(), text, "use")
        })
        .collect();
    edits.push(RefactorEdit::new(
        line_start(source, declaration.start_byte()),
        line_end(source, declaration.end_byte()),
        String::new(),
        "declaration",
    ));
    Some(Ok(Plan::new(id, label, edits).named(name.to_string())))
}

/// The declarator that introduces `name` in this method.
fn declarator_of<'t>(method: &Node<'t>, name: &str, source: &str) -> Option<Node<'t>> {
    descendants(*method, "variable_declarator").into_iter().find(|d| {
        d.child_by_field_name("name").map(|n| text(&n, source)) == Some(name)
    })
}

fn is_parameter(method: &Node<'_>, name: &str, source: &str) -> bool {
    method
        .child_by_field_name("parameters")
        .map(|params| {
            descendants(params, "identifier").iter().any(|n| text(n, source) == name)
        })
        .unwrap_or(false)
}

/// Whether this identifier IS a declaration's name rather than a use of one.
fn is_declaration_name(node: &Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        matches!(p.kind(), "variable_declarator" | "formal_parameter" | "catch_formal_parameter")
            && p.child_by_field_name("name").map(|n| n.id()) == Some(node.id())
    })
}

/// The reason this cannot be inlined, if there is one. See the module docs for the three.
fn unsafe_to_inline(
    method: &Node<'_>,
    declaration: &Node<'_>,
    value: &Node<'_>,
    name: &str,
    uses: &[Node<'_>],
    source: &str,
) -> Option<&'static str> {
    // `int[] t = {1, 2};` — the braces are declaration syntax, not an expression, so the value
    // cannot be moved anywhere the name was. `new int[]{1, 2}` can, and is a different text.
    if value.kind() == "array_initializer" {
        return Some(
            "an array written with braces is declaration syntax and cannot be moved into an expression",
        );
    }
    if is_assigned(method, name, source) {
        return Some("this variable is assigned again later, so its value is not the one written here");
    }
    if uses.len() > 1 && !is_pure(value) {
        return Some(
            "the value has a side effect and is read more than once — inlining would run it twice",
        );
    }
    // A capture that moves: something the initialiser reads is reassigned between the declaration
    // and a use, so evaluating it there gives a different answer.
    let after = declaration.end_byte();
    for read in identifiers(*value) {
        let captured = text(&read, source);
        if captured == name {
            continue;
        }
        if assigned_between(method, captured, after, source) {
            return Some(
                "the value reads a variable that changes before this is used, so moving it would \
                 change what it computes",
            );
        }
    }
    None
}

/// Whether `name` is the target of an assignment or an increment anywhere in the method.
fn is_assigned(method: &Node<'_>, name: &str, source: &str) -> bool {
    assignment_targets(method, source).iter().any(|(target, _)| target == name)
}

/// Whether `name` is assigned at a byte offset at or after `from`.
fn assigned_between(method: &Node<'_>, name: &str, from: usize, source: &str) -> bool {
    assignment_targets(method, source)
        .iter()
        .any(|(target, at)| target == name && *at >= from)
}

/// Every `(name, offset)` a method assigns to — plain assignment, compound assignment, `++`/`--`.
fn assignment_targets(method: &Node<'_>, source: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for kind in ["assignment_expression", "update_expression"] {
        for node in descendants(*method, kind) {
            let target = node
                .child_by_field_name("left")
                .or_else(|| node.named_child(0))
                .map(|n| text(&n, source).to_string());
            if let Some(target) = target {
                out.push((target, node.start_byte()));
            }
        }
    }
    out
}

/// Whether an expression only reads — no call, no `new`, no assignment, no increment.
///
/// Conservative on purpose: an unknown node kind counts as impure. Being wrong the other way means
/// silently running somebody's `next()` twice.
fn is_pure(expr: &Node<'_>) -> bool {
    match expr.kind() {
        "method_invocation" | "object_creation_expression" | "array_creation_expression"
        | "assignment_expression" | "update_expression" | "switch_expression"
        | "lambda_expression" => false,
        "identifier" | "this" | "field_access" | "string_literal" | "character_literal"
        | "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal"
        | "binary_integer_literal" | "decimal_floating_point_literal"
        | "hex_floating_point_literal" | "true" | "false" | "null_literal" | "class_literal" => true,
        "binary_expression" | "unary_expression" | "parenthesized_expression" | "cast_expression"
        | "ternary_expression" | "instanceof_expression" | "array_access" => {
            let mut cursor = expr.walk();
            let all = expr.named_children(&mut cursor).all(|c| is_pure(&c));
            all
        }
        _ => false,
    }
}

/// Whether the value needs wrapping where it is going.
///
/// Decided by the **kinds** of the two nodes rather than by counting operators: a compound
/// expression dropped anywhere that binds tighter than it does needs parentheses, and a primary —
/// a name, a literal, a call, an already-parenthesised expression — never does.
fn needs_parentheses(value: &Node<'_>, at: &Node<'_>) -> bool {
    let compound = matches!(
        value.kind(),
        "binary_expression"
            | "ternary_expression"
            | "assignment_expression"
            | "instanceof_expression"
            | "lambda_expression"
            | "cast_expression"
            | "unary_expression"
    );
    if !compound {
        return false;
    }
    let Some(parent) = at.parent() else { return false };
    // Somewhere the expression stands alone: an argument, an initialiser, the whole of a statement.
    // Wrapping there is noise.
    let standalone = matches!(
        parent.kind(),
        "argument_list"
            | "expression_statement"
            | "variable_declarator"
            | "return_statement"
            | "parenthesized_expression"
            | "array_initializer"
            | "yield_statement"
            | "assert_statement"
    );
    !standalone && is_expression(&parent)
}

/// The start of the line `offset` is on — so deleting a declaration takes its indentation with it.
fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Just past the end of the line `offset` is on, newline included.
fn line_end(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[offset..].find('\n').map(|i| offset + i + 1).unwrap_or(source.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    /// The caret one byte into `needle` — enough to land inside the name every test points at.
    fn run(source: &str, needle: &str) -> Outcome {
        run_at(source, needle, 1)
    }

    fn run_at(source: &str, needle: &str, delta: usize) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap() + delta;
        inline_variable(tree.root_node(), source, at)
    }

    #[test]
    fn a_single_use_local_disappears_into_its_use() {
        let source = "class A {\n    int f(int a, int b) {\n        int sum = a + b;\n        return sum;\n    }\n}";
        let Some(Ok(plan)) = run(source, "sum = a") else { panic!("no plan") };
        assert_eq!(plan.apply(source), "class A {\n    int f(int a, int b) {\n        return a + b;\n    }\n}");
    }

    /// The parenthesisation that makes this safe rather than clever.
    #[test]
    fn a_compound_value_is_wrapped_where_the_context_binds_tighter() {
        let source = "class A {\n    int f(int a, int b) {\n        int sum = a + b;\n        return sum * 2;\n    }\n}";
        let Some(Ok(plan)) = run(source, "sum = a") else { panic!("no plan") };
        assert!(plan.apply(source).contains("return (a + b) * 2;"), "{}", plan.apply(source));
    }

    /// …and not wrapped where it stands alone, which would be noise.
    #[test]
    fn a_value_standing_alone_is_not_wrapped() {
        let source = "class A {\n    void f(int a, int b) {\n        int sum = a + b;\n        take(sum);\n    }\n    void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "sum = a") else { panic!("no plan") };
        assert!(plan.apply(source).contains("take(a + b);"), "{}", plan.apply(source));
    }

    #[test]
    fn a_reassigned_variable_is_refused() {
        let source = "class A {\n    int f() {\n        int x = 1;\n        x = 2;\n        return x;\n    }\n}";
        let Some(Err(refusal)) = run(source, "x = 1") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("assigned again"), "{}", refusal.reason);
    }

    /// The one that a search-and-replace gets wrong in silence.
    #[test]
    fn a_value_reading_something_that_changes_later_is_refused() {
        let source = "class A {\n    int f(int n) {\n        int doubled = n * 2;\n        n = 5;\n        return doubled;\n    }\n}";
        let Some(Err(refusal)) = run(source, "doubled = n") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("changes before"), "{}", refusal.reason);
    }

    #[test]
    fn a_side_effect_read_twice_is_refused() {
        let source = "class A {\n    void f() {\n        int n = next();\n        take(n);\n        take(n);\n    }\n    int next() { return 1; }\n    void take(int x) {}\n}";
        let Some(Err(refusal)) = run(source, "n = next") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("side effect"), "{}", refusal.reason);
    }

    /// …but once is fine, and is exactly the case worth inlining.
    #[test]
    fn a_side_effect_read_once_is_allowed() {
        let source = "class A {\n    void f() {\n        int n = next();\n        take(n);\n    }\n    int next() { return 1; }\n    void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "n = next") else { panic!("no plan") };
        assert!(plan.apply(source).contains("take(next());"), "{}", plan.apply(source));
    }

    /// Regression: the caret one byte PAST the name. `int x = 1;` with the caret after `x` used to
    /// find no identifier at all — the node at that offset is the declarator, not the name — so
    /// inline was silently unavailable wherever a user clicked at the end of a word.
    #[test]
    fn a_caret_at_the_end_of_the_name_is_still_on_it() {
        let source = "class A {\n    int f() {\n        int x = 1;\n        return x + 1;\n    }\n}";
        let at = source.find("x = 1").unwrap() + 1; // immediately after `x`
        let tree = parse_java(source).unwrap();
        let Some(Ok(plan)) = inline_variable(tree.root_node(), source, at) else {
            panic!("expected a plan")
        };
        assert!(plan.apply(source).contains("return 1 + 1;"), "{}", plan.apply(source));
    }

    /// Regression: `int[] t = {1, 2};` inlined into `return t;` gives `return {1, 2};`, which is
    /// not an expression at all.
    #[test]
    fn an_array_initialiser_cannot_be_moved_into_an_expression() {
        let source = "class A {\n    int[] f() {\n        int[] types = {1, 2};\n        return types;\n    }\n}";
        let Some(Err(refusal)) = run(source, "types = {") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("declaration syntax"), "{}", refusal.reason);
    }

    #[test]
    fn a_parameter_says_why_it_cannot_be_inlined() {
        let source = "class A {\n    int f(int a) {\n        return a;\n    }\n}";
        // The caret on the parameter's own name, which is the wrong guess this refusal exists for.
        let Some(Err(refusal)) = run_at(source, "int a)", 4) else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("parameter"), "{}", refusal.reason);
    }

    #[test]
    fn a_multi_declarator_statement_is_refused_rather_than_half_deleted() {
        let source = "class A {\n    int f() {\n        int x = 1, y = 2;\n        return x + y;\n    }\n}";
        let Some(Err(refusal)) = run(source, "x = 1") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("more than one variable"), "{}", refusal.reason);
    }
}
