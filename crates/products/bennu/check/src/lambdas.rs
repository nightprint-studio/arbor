//! Captured-variable mutation in a lambda — a local captured by a lambda must be *effectively
//! final*, so assigning to it from inside the lambda is a compile error (JLS §15.27.2).
//!
//! Pure-AST, and deliberately conservative to avoid the field trap: we only flag an assignment /
//! `++`/`--` whose target is a **bare identifier that is a local variable (or parameter) of the
//! ENCLOSING method** — never a field (`this.x`, or a bare field name, which is legal to mutate in a
//! lambda) and never the lambda's own parameters / locals. A field and a local can share a name, so
//! restricting to known enclosing-method locals is what keeps this false-positive-free.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// All "modifies a captured local" errors in `root`.
pub fn lambda_capture_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    lambda_capture_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn lambda_capture_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "lambda_expression" {
            continue;
        }
        check_lambda(n, bytes, &mut out);
    }
    out
}

fn check_lambda(lambda: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = lambda.child_by_field_name("body") else { return };

    // Names local to the enclosing method (params + local declarations) — the capturable set.
    let Some(method) = enclosing_callable(lambda) else { return };
    let mut enclosing: HashSet<String> = HashSet::new();
    if let Some(params) = method.child_by_field_name("parameters") {
        collect_param_names(params, bytes, &mut enclosing);
    }
    collect_local_names(method, bytes, &mut enclosing);

    // Names the lambda introduces itself (its params + its own locals) — assigning to these is fine.
    let mut own: HashSet<String> = HashSet::new();
    if let Some(params) = lambda.child_by_field_name("parameters") {
        collect_lambda_param_names(params, bytes, &mut own);
    }
    collect_local_names(body, bytes, &mut own);

    // Walk the lambda body for assignments / updates targeting a captured local.
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        // Don't descend into a NESTED lambda — its own captures are checked when we reach it.
        if n.kind() == "lambda_expression" && n.id() != lambda.id() {
            continue;
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        let target = match n.kind() {
            "assignment_expression" => n.child_by_field_name("left"),
            "update_expression" => n.named_child(0),
            _ => None,
        };
        let Some(t) = target else { continue };
        if t.kind() != "identifier" {
            continue; // a field access (`this.x`, `obj.x`) is legal to mutate — not a captured local
        }
        let Ok(name) = t.utf8_text(bytes) else { continue };
        if enclosing.contains(name) && !own.contains(name) {
            out.push(Diagnostic {
                message: format!(
                    "Local variable `{name}` is modified in a lambda; captured variables must be final or effectively final"
                ),
                severity: "error".to_string(),
                start: t.start_byte(),
                end: t.end_byte(),
            });
        }
    }
}

/// The nearest enclosing method / constructor / lambda of `node` (the scope whose locals it can
/// capture). Stops at a type declaration (a field initializer has no capturable method locals).
fn enclosing_callable(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "method_declaration" | "constructor_declaration" | "lambda_expression" => return Some(n),
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" => return None,
            _ => {}
        }
        cur = n.parent();
    }
    None
}

fn collect_param_names(params: Node, bytes: &[u8], into: &mut HashSet<String>) {
    // `params` is a method/lambda `formal_parameters` node → `formal_parameter`/`spread_parameter`
    // → name. It holds only this callable's own parameters (no nested lambdas), so a plain walk is
    // precise.
    let mut stack = vec![params];
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "formal_parameter" | "spread_parameter") {
            if let Some(name) = n.child_by_field_name("name") {
                if let Ok(t) = name.utf8_text(bytes) {
                    into.insert(t.to_string());
                }
            }
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
    }
}

fn collect_local_names(node: Node, bytes: &[u8], into: &mut HashSet<String>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "local_variable_declaration" {
            let mut c = n.walk();
            for d in n.named_children(&mut c) {
                if d.kind() == "variable_declarator" {
                    if let Some(name) = d.child_by_field_name("name") {
                        if let Ok(t) = name.utf8_text(bytes) {
                            into.insert(t.to_string());
                        }
                    }
                }
            }
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
    }
}

fn collect_lambda_param_names(params: Node, bytes: &[u8], into: &mut HashSet<String>) {
    // A lambda's parameters can be a single bare `identifier`, an `inferred_parameters` list, or a
    // `formal_parameters` list.
    match params.kind() {
        "identifier" => {
            if let Ok(t) = params.utf8_text(bytes) {
                into.insert(t.to_string());
            }
        }
        _ => {
            let mut stack = vec![params];
            while let Some(n) = stack.pop() {
                if n.kind() == "identifier" {
                    if let Ok(t) = n.utf8_text(bytes) {
                        into.insert(t.to_string());
                    }
                }
                if matches!(n.kind(), "formal_parameter" | "spread_parameter") {
                    if let Some(name) = n.child_by_field_name("name") {
                        if let Ok(t) = name.utf8_text(bytes) {
                            into.insert(t.to_string());
                        }
                    }
                    continue;
                }
                let mut c = n.walk();
                for ch in n.named_children(&mut c) {
                    stack.push(ch);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(members: &str) -> Vec<String> {
        let src = format!("import java.util.List; class C {{ {members} }}");
        let tree = parse(&src);
        lambda_capture_errors(tree.root_node(), &src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn modifying_captured_local_is_flagged() {
        let e = errs("void m(List<String> xs) { int n = 0; xs.forEach(s -> n++); }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`n`"), "{e:?}");
    }

    #[test]
    fn assigning_captured_local_is_flagged() {
        let e = errs("void m(List<String> xs) { String last = null; xs.forEach(s -> last = s); }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`last`"));
    }

    #[test]
    fn modifying_lambda_own_local_is_ok() {
        assert!(errs("void m(List<String> xs) { xs.forEach(s -> { int c = 0; c++; }); }").is_empty());
    }

    #[test]
    fn modifying_lambda_param_is_ok() {
        // Reassigning the lambda's own parameter is not a capture.
        assert!(errs("void m(List<Integer> xs) { xs.forEach(s -> { s = 0; }); }").is_empty());
    }

    #[test]
    fn modifying_a_field_in_lambda_is_ok() {
        // `count` is a FIELD, not a captured local — mutating it in a lambda is legal, must NOT flag.
        assert!(errs("int count; void m(List<String> xs) { xs.forEach(s -> count++); }").is_empty());
    }

    #[test]
    fn reading_captured_local_is_ok() {
        // Only MUTATION is an error; reading a captured local is fine.
        assert!(errs("void m(List<String> xs) { int n = 0; xs.forEach(s -> { int y = n + 1; }); }").is_empty());
    }
}
