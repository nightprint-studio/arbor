//! Where a pattern variable is, and is not, in scope (pure-AST, JLS §6.3.1).
//!
//! `if (o instanceof String s)` binds `s` only where the compiler can prove the test passed:
//!
//! | construct | `s` is in scope in… |
//! |---|---|
//! | `a && b` | `b`, when `a` binds it **when true** |
//! | `a \|\| b` | `b`, when `a` binds it **when false** (`!(o instanceof String s) \|\| s.isEmpty()`) |
//! | `!e` | swaps *when true* and *when false* |
//! | `if (e) S else T` | `S` for *when true*, `T` for *when false* |
//! | after `if (e) S` | only for *when false*, and only if `S` cannot complete normally |
//!
//! This check reports a **use** of the variable in a place those rules rule out: the right of
//! `o instanceof String s || s.isEmpty()`, the `else` branch of a positive test, the statements after an
//! `if` whose test binds only when true. It also reports a binding that re-declares a local or
//! parameter already in scope (`already.defined`).
//!
//! The scope rules are only half the question — a name that is out of scope may still mean something
//! else (a field, a static import, another binding of the same name), so a use is reported only when
//! the method declares the name nowhere else, the enclosing classes have no field of that name (and
//! the nearest has no superclass to inherit one from), and the file has no `static` import.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::decls::local_record::local_declared_before;
use crate::engine::check_id::CheckId;
use crate::support::nodes::child_field_name;
use crate::support::scopes::{enclosing_executable_scope, is_value_position};

/// Whether the variable is introduced when the tested expression is true / false.
#[derive(Clone, Copy)]
struct Introduced {
    when_true: bool,
    when_false: bool,
}

/// All pattern-variable scope errors over the shared pre-collected node list.
pub fn pattern_scope_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let has_static_import = nodes.iter().any(|n| n.kind() == "import_declaration" && n.utf8_text(bytes).is_ok_and(|t| t.contains("static")));
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "instanceof_expression" && !n.has_error()) {
        let Some(binding) = n.child_by_field_name("name") else { continue };
        let Ok(name) = binding.utf8_text(bytes) else { continue };
        let Some(method) = enclosing_executable_scope(n).filter(|m| m.kind() != "lambda_expression") else { continue };
        if local_declared_before(n, method, name, bytes) {
            out.push(CheckId::DuplicateDeclaration.at(
                binding,
                format!("Variable `{name}` is already defined in this scope, so a pattern cannot bind it again"),
            ));
            continue;
        }
        if has_static_import || !name_means_only_this_binding(method, binding, name, bytes) {
            continue;
        }
        for region in out_of_scope_regions(n) {
            report_uses(region, name, bytes, &mut out);
        }
    }
    out
}

/// The subtrees where the binding introduced by `instanceof` is provably NOT in scope.
fn out_of_scope_regions(instanceof: Node) -> Vec<Node> {
    let mut regions = Vec::new();
    let mut flow = Introduced { when_true: true, when_false: false };
    let mut cur = instanceof;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "parenthesized_expression" => {}
            "unary_expression" if is_operator(parent, "!") => {
                flow = Introduced { when_true: flow.when_false, when_false: flow.when_true };
            }
            "binary_expression" if is_operator(parent, "&&") => {
                // `a && b` binds when true what either side binds; it binds nothing when false.
                let left = parent.child_by_field_name("left").is_some_and(|l| l.id() == cur.id());
                if left && flow.when_false && !flow.when_true {
                    regions.extend(parent.child_by_field_name("right"));
                }
                flow = Introduced { when_true: flow.when_true, when_false: false };
            }
            "binary_expression" if is_operator(parent, "||") => {
                // `a || b` binds when false what either side binds; it binds nothing when true.
                let left = parent.child_by_field_name("left").is_some_and(|l| l.id() == cur.id());
                if left && flow.when_true && !flow.when_false {
                    regions.extend(parent.child_by_field_name("right"));
                }
                flow = Introduced { when_true: false, when_false: flow.when_false };
            }
            _ => break,
        }
        cur = parent;
    }
    let Some(statement) = cur.parent().filter(|p| {
        p.kind() == "if_statement" && p.child_by_field_name("condition").is_some_and(|c| c.id() == cur.id())
    }) else {
        return regions;
    };
    let alternative = statement.child_by_field_name("alternative");
    if !flow.when_false {
        regions.extend(alternative);
    }
    if !flow.when_true {
        regions.extend(statement.child_by_field_name("consequence"));
    }
    // After `if (e) S` the variable is in scope only if `e` binds it when false (and `S` cannot complete
    // normally). Without an `else`, a test that does not is followed by statements that cannot see it.
    if !flow.when_false && alternative.is_none() && statement.parent().is_some_and(|p| p.kind() == "block") {
        let mut next = statement.next_named_sibling();
        while let Some(following) = next {
            regions.push(following);
            next = following.next_named_sibling();
        }
    }
    regions
}

fn is_operator(expr: Node, operator: &str) -> bool {
    expr.child_by_field_name("operator").is_some_and(|op| op.kind() == operator)
}

/// Every value use of `name` under `region`.
fn report_uses(region: Node, name: &str, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut stack = vec![region];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && n.utf8_text(bytes) == Ok(name) && (is_value_position(n) || is_receiver(n)) {
            out.push(CheckId::UnresolvedSymbol.at(
                n,
                format!("Cannot find symbol `{name}` — the pattern variable is not in scope here"),
            ));
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

/// The head of a member access — `s` in `s.isEmpty()` and `s.length`. [`is_value_position`] leaves it
/// out because a head can be a type or a package as well as a variable; here the name is known to be
/// a variable's (nothing else in the method or its classes is called that, see
/// [`name_means_only_this_binding`]).
fn is_receiver(ident: Node) -> bool {
    ident.parent().is_some_and(|p| {
        matches!(p.kind(), "method_invocation" | "field_access")
            && child_field_name(p, ident).as_deref() == Some("object")
    })
}

/// Whether `name`, read anywhere in the method, can only mean the pattern `binding`: nothing else in the
/// method declares it, no enclosing class has a field of that name, and the nearest class has no
/// superclass to inherit one from.
fn name_means_only_this_binding(method: Node, binding: Node, name: &str, bytes: &[u8]) -> bool {
    if declared_elsewhere(method, binding, name, bytes) {
        return false;
    }
    let mut cur = method.parent();
    let mut nearest = true;
    while let Some(p) = cur {
        if matches!(p.kind(), "class_declaration" | "enum_declaration" | "record_declaration" | "interface_declaration") {
            if nearest && (p.child_by_field_name("superclass").is_some() || p.kind() != "class_declaration") {
                return false;
            }
            nearest = false;
            if let Some(body) = p.child_by_field_name("body") {
                if declares_field(body, name, bytes) {
                    return false;
                }
            }
        }
        cur = p.parent();
    }
    true
}

fn declares_field(body: Node, name: &str, bytes: &[u8]) -> bool {
    let mut c = body.walk();
    let found = body.named_children(&mut c).filter(|m| m.kind() == "field_declaration").any(|field| {
        let mut fc = field.walk();
        let declared = field.named_children(&mut fc).any(|d| {
            d.kind() == "variable_declarator"
                && d.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name)
        });
        declared
    });
    found
}

/// Whether anything in `method` other than `own` declares `name`: a parameter, a local, a lambda
/// parameter, a loop or catch variable, another pattern.
fn declared_elsewhere(method: Node, own: Node, name: &str, bytes: &[u8]) -> bool {
    let mut stack = vec![method];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && n.id() != own.id() && n.utf8_text(bytes) == Ok(name) && declares(n) {
            return true;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}

/// Whether an identifier stands where a name is introduced rather than used.
fn declares(ident: Node) -> bool {
    let Some(parent) = ident.parent() else { return false };
    match parent.kind() {
        "formal_parameter" | "spread_parameter" | "catch_formal_parameter" | "inferred_parameters" | "type_pattern"
        | "record_pattern_component" | "resource" => true,
        "variable_declarator" | "enhanced_for_statement" | "instanceof_expression" => {
            child_field_name(parent, ident).as_deref() == Some("name")
        }
        "lambda_expression" => child_field_name(parent, ident).as_deref() == Some("parameters"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        pattern_scope_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    fn body(statements: &str) -> String {
        format!("class C {{ boolean m(Object o) {{ {statements} }} }}")
    }

    #[test]
    fn a_use_on_the_wrong_side_of_or_is_flagged() {
        assert_eq!(messages(&body("return o instanceof String s || s.isEmpty();")).len(), 1);
        assert_eq!(messages(&body("return !(o instanceof String s) && s.isEmpty();")).len(), 1);
        assert!(messages(&body("return o instanceof String s && s.isEmpty();")).is_empty());
        assert!(messages(&body("return !(o instanceof String s) || s.isEmpty();")).is_empty());
    }

    #[test]
    fn the_else_branch_of_a_positive_test_does_not_see_the_variable() {
        let bad = body("if (o instanceof String s) { return true; } else { return s.isEmpty(); } ");
        assert_eq!(messages(&bad).len(), 1, "{:?}", messages(&bad));
        let ok = body("if (!(o instanceof String s)) { return false; } else { return s.isEmpty(); }");
        assert!(messages(&ok).is_empty());
    }

    #[test]
    fn statements_after_an_if_see_the_variable_only_when_it_binds_when_false() {
        let bad = body("if (o instanceof String s) { System.gc(); } return s.isEmpty();");
        assert_eq!(messages(&bad).len(), 1, "{:?}", messages(&bad));
        let ok = body("if (!(o instanceof String s)) { return false; } return s.isEmpty();");
        assert!(messages(&ok).is_empty());
    }

    #[test]
    fn a_name_that_means_something_else_is_left_alone() {
        // A field of the same name, a second binding, and an inherited field are not judged.
        assert!(messages("class C { String s; boolean m(Object o) { return o instanceof String s || s.isEmpty(); } }").is_empty());
        assert!(messages(&body("return o instanceof String s || o instanceof CharSequence s;")).is_empty());
        assert!(messages("class C extends B { boolean m(Object o) { return o instanceof String s || s.isEmpty(); } }").is_empty());
    }

    #[test]
    fn a_binding_that_redeclares_a_local_is_flagged() {
        let bad = body("String text = \"\"; if (o instanceof String text) { return true; } return false;");
        assert_eq!(messages(&bad).len(), 1, "{:?}", messages(&bad));
        assert!(messages(&body("if (o instanceof String text) { return true; } String other = \"\"; return false;")).is_empty());
    }
}
