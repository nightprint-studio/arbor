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
//! ## What counts as a use of the name
//!
//! Not "every identifier that spells it", which is what this used to gather and is how inlining a
//! local called `max` rewrote `Math.max(start, 0)` into `Math.(len1 - len2)(start, 0)`. Three
//! separate things wear the same word and none of them is a read of the variable:
//!
//! - a **method name** (`Math.max(…)`) and a **field selector** (`p.max`) — the name after the dot
//!   belongs to a different namespace entirely (JLS §6.5.1, "obscuring");
//! - a **binding occurrence** — a declarator, a parameter, a `catch` variable, a for-each variable,
//!   a pattern binding, and the one that produced `sorted.forEach(zoneNames[i] -> …)`: a **lambda
//!   parameter**, which is an identifier with no `formal_parameter` around it;
//! - a use of a **different variable of the same name** in a sibling or nested scope. Java lets a
//!   method declare `zoneName` twice in two different loops, and it lets an inner scope shadow an
//!   outer one; the uses that belong to THIS declaration are the ones inside its own scope, after
//!   it, and not under an inner declaration that took the name over.
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

    // The declaration this name belongs to — the one in scope AT THE CARET, not the first one the
    // method happens to declare. Two `for` loops each declaring `zoneName` are two variables, and
    // taking the first meant inlining a value from the wrong one.
    let Some(declarator) = declarator_in_scope(name_node, &method, name, source) else {
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

    // Every read of THIS variable: inside the scope it lives in, after its declaration, spelled the
    // same, actually a reference rather than a method name or a binding, and not taken over by an
    // inner declaration of the same name. See the module docs — each of those four was a defect.
    let scope = scope_of(declaration);
    let uses: Vec<Node<'_>> = identifiers(scope)
        .into_iter()
        .filter(|n| text(n, source) == name)
        .filter(|n| n.start_byte() >= declaration.end_byte())
        .filter(|n| is_a_reference(n))
        .filter(|n| !shadowed_between(*n, scope, name, source))
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

/// The declarator that introduces `name` **where `at` stands** — the innermost enclosing scope that
/// declares it, walking outward, which is what Java's own lookup does.
///
/// Searching the method for the first declarator of that name is what this used to do, and a method
/// is allowed to declare the same name in several disjoint scopes: `FastDateParser` has a
/// `zoneName` in one loop and another in the next, and the caret on the second one inlined the
/// first one's value.
fn declarator_in_scope<'t>(
    at: Node<'t>,
    method: &Node<'t>,
    name: &str,
    source: &str,
) -> Option<Node<'t>> {
    let mut node = Some(at);
    while let Some(n) = node {
        for declarator in descendants(n, "variable_declarator") {
            let same = declarator.child_by_field_name("name").map(|x| text(&x, source)) == Some(name);
            // Declared before the caret, and in THIS scope rather than in a nested one we happen to
            // contain (that one is a different variable, invisible here).
            if same
                && declarator.start_byte() <= at.start_byte()
                && scope_of(enclosing(declarator, &["local_variable_declaration"])?).id() == n.id()
            {
                return Some(declarator);
            }
        }
        if n.id() == method.id() {
            break;
        }
        node = n.parent();
    }
    None
}

/// The scope a declaration's name lives in: the block (or block-like construct) that contains it.
fn scope_of<'t>(declaration: Node<'t>) -> Node<'t> {
    let mut node = declaration.parent();
    while let Some(n) = node {
        if matches!(
            n.kind(),
            "block"
                | "constructor_body"
                | "switch_block"
                | "switch_block_statement_group"
                | "for_statement"
                | "enhanced_for_statement"
                | "try_with_resources_statement"
                | "lambda_expression"
        ) {
            return n;
        }
        node = n.parent();
    }
    declaration
}

/// Whether this identifier is a **reference to a variable** rather than something else wearing the
/// same word: a method name, the selector after a dot, or a binding occurrence.
fn is_a_reference(node: &Node<'_>) -> bool {
    let Some(parent) = node.parent() else { return true };
    let is_field = |field: &str| {
        parent.child_by_field_name(field).map(|n| n.id()) == Some(node.id())
    };
    match parent.kind() {
        // `Math.max(…)` — the name belongs to the method namespace, not the variable one.
        "method_invocation" if is_field("name") => false,
        // `p.max` — the selector is a member of whatever `p` is.
        "field_access" if is_field("field") => false,
        "scoped_identifier" if is_field("name") => false,
        // Every binding form. The lambda parameter is the one with no wrapper node of its own:
        // `x -> …` puts a bare `identifier` in the lambda's `parameters` field.
        "variable_declarator" | "formal_parameter" | "catch_formal_parameter"
        | "enhanced_for_statement" | "type_pattern" | "resource" | "labeled_statement"
            if is_field("name") =>
        {
            false
        }
        "lambda_expression" if is_field("parameters") => false,
        "inferred_parameters" => false,
        _ => true,
    }
}

/// Whether a scope between `use_node` and `scope` declares `name` itself, so the use reads THAT
/// variable and not ours.
fn shadowed_between(use_node: Node<'_>, scope: Node<'_>, name: &str, source: &str) -> bool {
    let mut node = use_node.parent();
    while let Some(n) = node {
        if n.id() == scope.id() {
            return false;
        }
        let binds = match n.kind() {
            "lambda_expression" => n
                .child_by_field_name("parameters")
                .is_some_and(|p| identifiers(p).iter().any(|i| text(i, source) == name)),
            "enhanced_for_statement" | "catch_formal_parameter" | "resource" | "type_pattern" => n
                .child_by_field_name("name")
                .is_some_and(|x| text(&x, source) == name),
            "block" | "for_statement" | "switch_block_statement_group" => {
                descendants(n, "variable_declarator").iter().any(|d| {
                    d.child_by_field_name("name").map(|x| text(&x, source)) == Some(name)
                        && d.start_byte() < use_node.start_byte()
                })
            }
            _ => false,
        };
        if binds {
            return true;
        }
        node = n.parent();
    }
    false
}

fn is_parameter(method: &Node<'_>, name: &str, source: &str) -> bool {
    method
        .child_by_field_name("parameters")
        .map(|params| {
            descendants(params, "identifier").iter().any(|n| text(n, source) == name)
        })
        .unwrap_or(false)
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
    // A narrowing the DECLARATION was allowed to make and an expression is not. `final byte n = 5;`
    // compiles because JLS §5.2 lets a constant that fits narrow in an assignment; the literal is
    // still an `int` everywhere else, so `f(n)` against an `f(byte)` becomes `f(5)` and stops
    // compiling. Eight of these in one file, all on `byte maxElementChars = 5;`.
    if narrows_at_the_declaration(declaration, value, source) {
        return Some(
            "the declared type narrows the value, which only an assignment may do — inlining it \
             would leave a wider expression where a narrower type is expected",
        );
    }
    // Moving the value INTO a lambda makes everything it reads a captured variable, and a captured
    // variable has to be effectively final. The guard below only looks at what changes AFTER the
    // declaration, which is the right question for the value; this is a different one, and its
    // answer is the whole method: `subarray` reassigns both its parameters before declaring
    // `newSize`, so `newSize` is fine and `endIndexExclusive - startIndexInclusive` is not.
    if uses.iter().any(|u| crosses_a_lambda(*u, declaration)) {
        for read in identifiers(*value) {
            let captured = text(&read, source);
            if captured != name && is_assigned(method, captured, source) {
                return Some(
                    "one of the values this reads is reassigned in this method, and a lambda may \
                     only capture a variable that never changes",
                );
            }
        }
    }
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

/// Whether the declaration's written type is a primitive NARROWER than the value's own type, so the
/// declaration is performing a narrowing that JLS §5.2 permits only there.
///
/// Syntactic on purpose and deliberately narrow: an integer literal is an `int`, and a `byte`,
/// `short` or `char` declared from one is the whole of the shape this has to catch. A cast, a
/// method call, or a variable of the right type is not a narrowing and is left alone.
fn narrows_at_the_declaration(declaration: &Node<'_>, value: &Node<'_>, source: &str) -> bool {
    let Some(ty) = declaration.child_by_field_name("type") else { return false };
    if !matches!(text(&ty, source).trim(), "byte" | "short" | "char") {
        return false;
    }
    is_integer_constant(value)
}

/// An integer literal, or one behind a sign or parentheses — the constant expressions §5.2 narrows.
fn is_integer_constant(expr: &Node<'_>) -> bool {
    match expr.kind() {
        "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal"
        | "binary_integer_literal" => true,
        "unary_expression" | "parenthesized_expression" => {
            expr.named_child(0).is_some_and(|c| is_integer_constant(&c))
        }
        _ => false,
    }
}

/// Whether reaching `use_node` from the declaration passes into a lambda or an anonymous class —
/// a body that CAPTURES what it reads instead of simply reading it.
fn crosses_a_lambda(use_node: Node<'_>, declaration: &Node<'_>) -> bool {
    let mut node = use_node.parent();
    while let Some(n) = node {
        if n.id() == declaration.id() || n.end_byte() < declaration.end_byte() {
            return false;
        }
        if matches!(n.kind(), "lambda_expression" | "class_body") {
            return true;
        }
        if n.kind() == "method_declaration" || n.kind() == "constructor_declaration" {
            return false;
        }
        node = n.parent();
    }
    false
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

    fn applied(source: &str, needle: &str) -> String {
        let Some(Ok(plan)) = run(source, needle) else { panic!("no plan for {needle}") };
        plan.apply(source)
    }

    /// The name after a dot belongs to another namespace. Inlining a local called `max` rewrote the
    /// `max` of `Math.max(start, 0)` and produced `Math.(len1 - len2)(start, 0)` — text that does
    /// not parse, which is the worst thing a refactoring can hand back.
    #[test]
    fn a_method_of_the_same_name_is_not_a_use() {
        let src = "class C {\n    int m(int a, int b) {\n        final int max = a - b;\n        int f = Math.max(a, 0);\n        return f + max;\n    }\n}";
        let out = applied(src, "max = a - b");
        assert!(out.contains("Math.max(a, 0)"), "{out}");
        assert!(out.contains("return f + (a - b);"), "{out}");
    }

    /// Two loops may each declare `zoneName`; they are two variables. Taking the first declarator in
    /// the method inlined the wrong value into the second one's uses.
    #[test]
    fn a_same_named_local_in_a_sibling_scope_is_a_different_variable() {
        let src = "class C {\n    void m(String[] names) {\n        for (int i = 0; i < 1; i++) {\n            final String zoneName = names[i];\n            use(zoneName);\n        }\n        for (int i = 0; i < 1; i++) {\n            final String zoneName = names[0];\n            use(zoneName);\n        }\n    }\n    void use(String s) {}\n}";
        let out = applied(src, "zoneName = names[0]");
        // The second loop lost its declaration and took its own value…
        assert!(out.contains("use(names[0]);"), "{out}");
        // …and the first one is untouched.
        assert!(out.contains("final String zoneName = names[i];"), "{out}");
        assert!(out.contains("use(zoneName);"), "{out}");
    }

    /// A lambda parameter is an identifier with no wrapper of its own, so nothing marked it as a
    /// binding: `sorted.forEach(zoneName -> f(zoneName))` came out `forEach(names[i] -> …)`.
    #[test]
    fn a_lambda_parameter_of_the_same_name_is_not_rewritten() {
        let src = "class C {\n    void m(java.util.List<String> rows, String[] names) {\n        final String row = names[0];\n        use(row);\n        rows.forEach(row -> use(row));\n    }\n    void use(String s) {}\n}";
        let out = applied(src, "row = names[0]");
        assert!(out.contains("use(names[0]);"), "{out}");
        assert!(out.contains("rows.forEach(row -> use(row));"), "{out}");
    }

    /// `final byte n = 5;` compiles because an assignment may narrow a constant that fits (JLS
    /// §5.2). The literal is an `int` everywhere else, so the value cannot simply move.
    #[test]
    fn a_declaration_that_narrows_a_constant_is_refused() {
        let src = "class C {\n    void m() {\n        final byte n = 5;\n        take(n);\n    }\n    void take(byte b) {}\n}";
        let Some(Err(refusal)) = run(src, "n = 5") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("narrow"), "{}", refusal.reason);
        // A `char` from a char literal is not a narrowing and still inlines.
        let ok = "class C {\n    void m() {\n        final char c = 'x';\n        take(c);\n    }\n    void take(char c) {}\n}";
        assert!(matches!(run(ok, "c = 'x'"), Some(Ok(_))), "a char literal narrows nothing");
    }

    /// Moving a value into a lambda makes what it reads a CAPTURE, and a capture must be effectively
    /// final. `subarray` reassigns both parameters before declaring the local, so the local is fine
    /// and the expression behind it is not.
    #[test]
    fn a_value_moved_into_a_lambda_may_only_read_effectively_final_locals() {
        let src = "class C {\n    int m(int lo, int hi) {\n        lo = lo + 1;\n        final int size = hi - lo;\n        return run(() -> size);\n    }\n    int run(java.util.function.IntSupplier s) { return 0; }\n}";
        let Some(Err(refusal)) = run(src, "size = hi - lo") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("never changes"), "{}", refusal.reason);
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
