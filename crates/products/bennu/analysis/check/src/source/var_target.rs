//! `var` target-type diagnostics (pure-AST).
//!
//! A `var` local infers its type FROM its initializer, so an initializer that has **no type of its
//! own** — a *poly expression* whose type depends on a target the `var` can't provide — is a compile
//! error. Four structurally-unambiguous shapes, all detectable from the syntax tree with no resolver:
//!
//!   * a **lambda** — `var f = () -> 1;` (JLS §14.4.1: "cannot infer type for local variable f");
//!   * a **method / constructor reference** — `var g = String::length;`;
//!   * an **array initializer** — `var xs = {1, 2, 3};` (only legal with an explicit array type);
//!   * the **`null` literal** — `var x = null;` (the null type is not denotable).
//!
//! PARAMOUNT — never a false positive. We flag ONLY when the declared type token is literally `var`
//! AND the declarator's DIRECT value is one of those four node kinds. A cast (`var r = (Runnable) () ->
//! {};`) wraps the lambda in a `cast_expression`, so the direct value is no longer a lambda → not
//! flagged (the cast supplies the target type). A non-`var` declaration is never touched.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// All `var` target-type errors over the shared pre-collected node list (one traversal across all
/// pure-AST checks).
pub fn var_target_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "local_variable_declaration" {
            check_declaration(n, bytes, &mut out);
        }
    }
    out
}

/// `var` where only a local variable may use it — a field, a method's return type or a parameter of a
/// method (`restricted.type.not.allowed.here`). From Java 10 `var` is a reserved type name there; below
/// it, `var` is an ordinary identifier and could name a class, so nothing is said.
pub fn var_position_errors_nodes(nodes: &[Node], source: &str, java_major: Option<u32>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !java_major.is_some_and(|m| m >= 10) {
        return out;
    }
    let bytes = source.as_bytes();
    for &n in nodes {
        let holder = match n.kind() {
            "field_declaration" | "method_declaration" => n.child_by_field_name("type"),
            // A parameter of a method or constructor; a lambda's `var x` is legal from Java 11.
            "formal_parameter" => n
                .parent()
                .filter(|p| p.kind() == "formal_parameters")
                .and_then(|p| p.parent())
                .filter(|owner| matches!(owner.kind(), "method_declaration" | "constructor_declaration"))
                .and_then(|_| n.child_by_field_name("type")),
            _ => None,
        };
        if let Some(ty) = holder.filter(|t| t.utf8_text(bytes) == Ok("var")) {
            out.push(crate::engine::check_id::CheckId::SyntaxError.at(
                ty,
                "`var` is not allowed here: it can only declare a local variable",
            ));
        }
    }
    out
}

fn check_declaration(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // SKIP unless the declared type is literally `var`. In tree-sitter-java a `var` declaration carries
    // its type as a `type_identifier` whose text is `var` (Lombok's back-ported `var` reads identically
    // — but a lambda/method-ref/array/null initializer is illegal under Lombok's `var` too, so this
    // stays sound). Any real type (`int`, `List<String>`) provides a target → nothing to infer wrongly.
    let Some(ty) = n.child_by_field_name("type") else { return };
    if ty.utf8_text(bytes) != Ok("var") {
        return;
    }
    // Each declarator: does its DIRECT value have no type of its own?
    let mut c = n.walk();
    for d in n.named_children(&mut c) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(val) = d.child_by_field_name("value") else {
            // `var none;` — nothing to infer from.
            out.push(crate::engine::check_id::CheckId::VarTypeInferenceFailed.at(
                d,
                "Cannot infer type for `var`: it needs an initializer",
            ));
            continue;
        };
        if let Some(reason) = untyped_initializer(val.kind()) {
            out.push(Diagnostic {
                message: format!("Cannot infer type for `var`: {reason}"),
                severity: crate::engine::check_id::CheckId::VarTypeInferenceFailed.severity().to_string(),
                code: crate::engine::check_id::CheckId::VarTypeInferenceFailed.code().to_string(),
                start: val.start_byte(),
                end: val.end_byte(),
            });
        }
    }
}

/// The reason `kind` has no self-standing type (→ illegal as a `var` initializer), or `None` when the
/// initializer is a normal typed expression. Only the four unambiguous poly / non-denotable shapes.
fn untyped_initializer(kind: &str) -> Option<&'static str> {
    match kind {
        "lambda_expression" => Some("a lambda needs an explicit target type"),
        "method_reference" => Some("a method reference needs an explicit target type"),
        "array_initializer" => Some("an array initializer needs an explicit array type"),
        "null_literal" => Some("the `null` type cannot be inferred"),
        _ => None,
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

    fn msgs(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        let tree = parse(&src);
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        var_target_errors_nodes(&nodes, &src).into_iter().map(|d| d.message).collect()
    }

    // ── positives ──────────────────────────────────────────────────────────────

    #[test]
    fn var_lambda_is_flagged() {
        let m = msgs("var f = () -> 1;");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("lambda") && m[0].contains("target type"), "{m:?}");
    }

    #[test]
    fn var_method_reference_is_flagged() {
        let m = msgs("var g = String::valueOf;");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("method reference"), "{m:?}");
    }

    #[test]
    fn var_array_initializer_is_flagged() {
        let m = msgs("var xs = {1, 2, 3};");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("array"), "{m:?}");
    }

    #[test]
    fn var_null_is_flagged() {
        let m = msgs("var x = null;");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("null"), "{m:?}");
    }

    // ── negatives (must NEVER flag) ────────────────────────────────────────────

    #[test]
    fn var_with_typed_initializer_is_ok() {
        assert!(msgs("var n = 5;").is_empty());
        assert!(msgs("var s = \"x\";").is_empty());
        assert!(msgs("var list = new java.util.ArrayList<String>();").is_empty());
    }

    #[test]
    fn var_lambda_behind_a_cast_is_ok() {
        // The cast supplies the target type, so the direct value is a `cast_expression`, not a lambda.
        assert!(msgs("var r = (Runnable) () -> {};").is_empty(), "{:?}", msgs("var r = (Runnable) () -> {};"));
    }

    #[test]
    fn explicitly_typed_lambda_is_ok() {
        // A real target type is present → nothing for us to flag.
        assert!(msgs("java.util.function.Supplier<Integer> f = () -> 1;").is_empty());
        assert!(msgs("Object x = null;").is_empty());
        assert!(msgs("int[] xs = {1, 2, 3};").is_empty());
    }

    #[test]
    fn var_is_only_for_locals_and_only_reserved_from_java_10() {
        let count = |src: &str, major| {
            let tree = parse(src);
            let nodes = crate::engine::check::collect_nodes(tree.root_node());
            var_position_errors_nodes(&nodes, src, major).len()
        };
        assert_eq!(count("class A { var f = 1; }", Some(17)), 1);
        assert_eq!(count("class A { var m() { return 1; } }", Some(17)), 1);
        assert_eq!(count("class A { void m(var p) { } }", Some(17)), 1);
        assert_eq!(count("class A { var f = 1; }", Some(8)), 0);
        assert_eq!(count("class A { void m() { var x = 1; java.util.function.Function<String,String> f = (var s) -> s; } }", Some(17)), 0);
    }

    #[test]
    fn var_without_initializer_cannot_infer_a_type() {
        // javac reports `var x;` as `cant.infer.local.var.type`, the same key as the other shapes.
        assert_eq!(msgs("var x;").len(), 1);
        assert!(msgs("var x = 1;").is_empty());
    }
}
