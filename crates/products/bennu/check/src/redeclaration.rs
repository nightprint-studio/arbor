//! Redeclaration diagnostics (pure-AST): the same name declared twice where Java forbids it —
//! two **fields** in a type, two **parameters** of a method/constructor/lambda, two **local
//! variables** in one block, or two **types** with the same name in one scope (a compilation unit or
//! an enclosing type). Duplicate *method/constructor signatures* live in [`crate::duplicates`].
//!
//! Every comparison is exact-name within a single lexical scope, so it can never be a false positive.
//! Locals are compared only within the **same immediate block** (a legal redeclaration in a disjoint
//! sibling scope — two `for` loops each declaring `i` — is never flagged, since those declarations
//! aren't direct children of one block).

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Parse `source` and flag illegal redeclarations.
pub fn redeclaration_errors(source: &str) -> Vec<Diagnostic> {
    match bennu_java::prelude::parse_java(source) {
        Some(tree) => redeclaration_errors_in(tree.root_node(), source),
        None => Vec::new(),
    }
}

/// Tree-driven core (shared with the `check_file` single-parse path).
pub fn redeclaration_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    redeclaration_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks). The
/// old pre-loop "top-level types share the compilation-unit scope" call is folded into the loop via
/// the `program` node (the first entry in the slice).
pub fn redeclaration_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // The compilation unit's top-level types share one scope.
            "program" => check_type_dups(n, bytes, &mut out),
            "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
            | "annotation_type_declaration" => {
                if let Some(body) = n.child_by_field_name("body") {
                    check_field_dups(body, bytes, &mut out);
                    check_type_dups(body, bytes, &mut out); // nested sibling types
                }
            }
            "method_declaration" | "constructor_declaration" | "lambda_expression" => {
                check_param_dups(n, bytes, &mut out);
                if n.kind() == "lambda_expression" {
                    check_lambda_shadowing(n, bytes, &mut out);
                }
            }
            "block" => check_local_dups(n, bytes, &mut out),
            _ => {}
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// Flag a field whose name repeats one already declared in the same type body.
fn check_field_dups(body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        if matches!(m.kind(), "field_declaration" | "constant_declaration") {
            flag_declarator_dups(m, bytes, &mut seen, "field", out);
        }
    }
}

/// Flag a local variable whose name repeats one already declared in the same block.
fn check_local_dups(block: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut c = block.walk();
    for s in block.named_children(&mut c) {
        if s.kind() == "local_variable_declaration" {
            flag_declarator_dups(s, bytes, &mut seen, "variable", out);
        }
    }
}

/// Collect each `variable_declarator` name of a declaration; a name already in `seen` is a
/// redeclaration.
fn flag_declarator_dups(
    decl: Node,
    bytes: &[u8],
    seen: &mut HashSet<String>,
    what: &str,
    out: &mut Vec<Diagnostic>,
) {
    let mut c = decl.walk();
    for d in decl.named_children(&mut c) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = d.child_by_field_name("name") else { continue };
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        if !seen.insert(name.to_string()) {
            out.push(err(format!("Duplicate {what} `{name}`"), name_node));
        }
    }
}

/// Flag a parameter whose name repeats another in the same parameter list.
fn check_param_dups(member: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // A method / constructor uses `parameters`; a lambda may use `parameters` too (a single bare
    // identifier lambda param can't collide, so it's ignored).
    let Some(params) = member.child_by_field_name("parameters") else { return };
    let mut seen: HashSet<String> = HashSet::new();
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        let name_node = match p.kind() {
            "formal_parameter" | "spread_parameter" => p.child_by_field_name("name"),
            // A lambda `inferred_parameters` lists bare identifiers.
            "identifier" => Some(p),
            _ => None,
        };
        let Some(name_node) = name_node else { continue };
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        if !seen.insert(name.to_string()) {
            out.push(err(format!("Duplicate parameter `{name}`"), name_node));
        }
    }
}

/// Flag a lambda parameter that shadows a name already in scope where the lambda is written.
///
/// A lambda body shares the enclosing scope rather than opening a class-like one, so its parameters
/// may not re-use a local, a parameter, or an outer lambda's parameter (JLS §6.4) — javac calls it
/// `already.defined`. `list.stream().filter(it -> other(it).anyMatch(it -> …))` is the shape that
/// reaches production, and the reading is genuinely ambiguous to a person too.
///
/// A FIELD may legally be shadowed, which is why only executable scopes are searched and the climb
/// stops at a type boundary — a lambda inside an anonymous class body does not see the method the
/// anonymous class is written in.
///
/// Deliberately narrow, because this must never be a false positive: only names a scope declares
/// **before** the lambda, and only from the shapes below. A local declared in a sibling block that
/// has already closed is not in scope and is not collected, since the climb only visits the
/// lambda's own ancestors.
fn check_lambda_shadowing(lambda: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut params: Vec<(String, Node)> = Vec::new();
    collect_lambda_params(lambda, bytes, &mut params);
    if params.is_empty() {
        return;
    }
    let mut outer: HashSet<String> = HashSet::new();
    let mut cur = lambda.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            break; // a type body is a new scope — what is above it may be shadowed
        }
        collect_declared_before(n, lambda, bytes, &mut outer);
        if matches!(n.kind(), "method_declaration" | "constructor_declaration") {
            break; // the outermost executable scope
        }
        cur = n.parent();
    }
    for (name, node) in params {
        if outer.contains(&name) {
            out.push(err(
                format!("Lambda parameter `{name}` is already defined in the enclosing scope"),
                node,
            ));
        }
    }
}

/// The names `scope` declares that are in scope AT `lambda`: its parameters, and the local
/// declarations that end before the lambda begins.
fn collect_declared_before(
    scope: Node,
    lambda: Node,
    bytes: &[u8],
    into: &mut HashSet<String>,
) {
    match scope.kind() {
        // Parameters are in scope for the whole body, wherever the lambda sits in it. A lambda's
        // own parameters read the same way, and `collect_lambda_params` is the one reading of the
        // three shapes they come in.
        "method_declaration" | "constructor_declaration" => {
            if let Some(p) = scope.child_by_field_name("parameters") {
                collect_names_under(p, bytes, into);
            }
        }
        "lambda_expression" => {
            let mut v = Vec::new();
            collect_lambda_params(scope, bytes, &mut v);
            into.extend(v.into_iter().map(|(n, _)| n));
        }
        // A local is in scope from its declaration onward — so only the ones that CLOSE before the
        // lambda starts. Direct children only: a declaration nested deeper is in an inner block.
        "block" | "constructor_body" => {
            let mut c = scope.walk();
            for ch in scope.named_children(&mut c) {
                if ch.kind() == "local_variable_declaration" && ch.end_byte() <= lambda.start_byte()
                {
                    collect_declarator_names(ch, bytes, into);
                }
            }
        }
        "for_statement" => {
            if let Some(init) = scope.child_by_field_name("init") {
                collect_declarator_names(init, bytes, into);
            }
        }
        "enhanced_for_statement" => {
            if let Some(nm) = scope.child_by_field_name("name") {
                if let Ok(t) = nm.utf8_text(bytes) {
                    into.insert(t.to_string());
                }
            }
        }
        "catch_clause" => {
            // The grammar gives the parameter no field name here, so it is found by KIND.
            let mut c = scope.walk();
            for ch in scope.named_children(&mut c) {
                if ch.kind() == "catch_formal_parameter" {
                    collect_names_under(ch, bytes, into);
                }
            }
        }
        "try_with_resources_statement" => {
            if let Some(r) = scope.child_by_field_name("resources") {
                collect_declarator_names(r, bytes, into);
            }
        }
        _ => {}
    }
}

/// The parameters a lambda declares, with the node to point at.
fn collect_lambda_params<'t>(
    lambda: Node<'t>,
    bytes: &[u8],
    out: &mut Vec<(String, Node<'t>)>,
) {
    let Some(params) = lambda.child_by_field_name("parameters") else { return };
    // `x -> …` gives a bare `identifier`; `(a, b) -> …` an `inferred_parameters`; `(String a) -> …`
    // a `formal_parameters`.
    let mut stack = vec![params];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "identifier" => {
                if let Ok(t) = n.utf8_text(bytes) {
                    out.push((t.to_string(), n));
                }
            }
            "formal_parameter" | "spread_parameter" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if let Ok(t) = nm.utf8_text(bytes) {
                        out.push((t.to_string(), nm));
                    }
                }
            }
            _ => {
                let mut c = n.walk();
                stack.extend(n.named_children(&mut c));
            }
        }
    }
}

/// Every `identifier` a parameter-ish subtree names.
fn collect_names_under(node: Node, bytes: &[u8], into: &mut HashSet<String>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "formal_parameter" | "spread_parameter" | "catch_formal_parameter" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if let Ok(t) = nm.utf8_text(bytes) {
                        into.insert(t.to_string());
                    }
                }
            }
            "identifier" => {
                if let Ok(t) = n.utf8_text(bytes) {
                    into.insert(t.to_string());
                }
            }
            _ => {
                let mut c = n.walk();
                stack.extend(n.named_children(&mut c));
            }
        }
    }
}

/// The names a `local_variable_declaration` / resource list binds (its `variable_declarator`s).
fn collect_declarator_names(node: Node, bytes: &[u8], into: &mut HashSet<String>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "variable_declarator" | "resource") {
            if let Some(nm) = n.child_by_field_name("name") {
                if let Ok(t) = nm.utf8_text(bytes) {
                    into.insert(t.to_string());
                }
            }
            continue;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

/// Flag a type declaration whose simple name repeats another in the same scope (compilation unit or
/// enclosing type body).
fn check_type_dups(container: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut c = container.walk();
    for ch in container.named_children(&mut c) {
        if !matches!(
            ch.kind(),
            "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
                | "annotation_type_declaration"
        ) {
            continue;
        }
        let Some(name_node) = ch.child_by_field_name("name") else { continue };
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        if !seen.insert(name.to_string()) {
            out.push(err(format!("Duplicate type `{name}` in the same scope"), name_node));
        }
    }
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic {
        message,
        severity: crate::check_id::CheckId::DuplicateDeclaration.severity().to_string(),
        code: crate::check_id::CheckId::DuplicateDeclaration.code().to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errs(src: &str) -> Vec<String> {
        redeclaration_errors(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn duplicate_field_is_flagged() {
        let d = errs("class C { int a; String a; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("field `a`"), "{d:?}");
    }

    #[test]
    fn duplicate_declarator_in_one_field_is_flagged() {
        assert_eq!(errs("class C { int a, a; }").len(), 1);
    }

    #[test]
    fn distinct_fields_are_ok() {
        assert!(errs("class C { int a; int b; }").is_empty());
    }

    #[test]
    fn duplicate_parameter_is_flagged() {
        let d = errs("class C { void m(int x, String x) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("parameter `x`"), "{d:?}");
    }

    #[test]
    fn duplicate_local_in_same_block_is_flagged() {
        let d = errs("class C { void m() { int x = 1; int x = 2; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("variable `x`"), "{d:?}");
    }

    #[test]
    fn same_local_name_in_disjoint_scopes_is_ok() {
        // Two `for` loops each declaring `i` — separate scopes, legal.
        let src = "class C { void m() { for (int i=0;i<1;i++){} for (int i=0;i<1;i++){} } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn local_shadowing_a_field_is_ok() {
        // A local may legally share a field's name.
        assert!(errs("class C { int x; void m() { int x = 1; } }").is_empty());
    }

    #[test]
    fn duplicate_top_level_type_is_flagged() {
        let d = errs("class A {} class A {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("type `A`"), "{d:?}");
    }

    #[test]
    fn class_and_interface_same_name_is_flagged() {
        assert_eq!(errs("class A {} interface A {}").len(), 1);
    }

    #[test]
    fn distinct_top_level_types_are_ok() {
        assert!(errs("class A {} class B {}").is_empty());
    }
}

#[cfg(test)]
mod lambda_shadow_tests {
    use super::redeclaration_errors;

    fn msgs(src: &str) -> Vec<String> {
        redeclaration_errors(src).into_iter().map(|d| d.message).collect()
    }

    /// The shape that reaches production: a nested lambda re-using the outer one's parameter name.
    /// javac refuses it (`already.defined`), and the reading is ambiguous to a person too.
    #[test]
    fn a_nested_lambda_may_not_reuse_the_outer_lambdas_parameter() {
        let src = r#"class A {
            boolean m(java.util.List<String> xs, String k) {
                return xs.stream().anyMatch(it -> java.util.Arrays.stream(it.split(",")).anyMatch(it -> it.equals(k)));
            }
        }"#;
        let m = msgs(src);
        assert!(
            m.iter().any(|s| s.contains("Lambda parameter `it` is already defined")),
            "{m:?}"
        );
    }

    /// Two lambdas that are SIBLINGS, not nested: neither is in the other's scope.
    #[test]
    fn two_sibling_lambdas_may_share_a_parameter_name() {
        let src = r#"class A {
            void m(java.util.List<String> xs) {
                xs.forEach(it -> use(it));
                xs.forEach(it -> use(it));
            }
        }"#;
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    /// The enclosing METHOD's parameter counts too.
    #[test]
    fn a_lambda_may_not_reuse_the_enclosing_methods_parameter() {
        let src = r#"class A {
            void m(String s, java.util.List<String> xs) { xs.forEach(s -> use(s)); }
        }"#;
        assert!(msgs(src).iter().any(|m| m.contains("`s` is already defined")), "{:?}", msgs(src));
    }

    /// So does a local declared before it.
    #[test]
    fn a_lambda_may_not_reuse_a_local_declared_before_it() {
        let src = r#"class A {
            void m(java.util.List<String> xs) { String v = "a"; xs.forEach(v -> use(v)); }
        }"#;
        assert!(msgs(src).iter().any(|m| m.contains("`v` is already defined")), "{:?}", msgs(src));
    }

    /// A local declared AFTER the lambda is not in scope at it.
    #[test]
    fn a_local_declared_after_the_lambda_is_not_shadowed() {
        let src = r#"class A {
            void m(java.util.List<String> xs) { xs.forEach(v -> use(v)); String v = "a"; }
        }"#;
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    /// A FIELD may legally be shadowed by a lambda parameter — the case that would make this check
    /// unusable if it were wrong, since every DTO field name is a plausible lambda parameter.
    #[test]
    fn a_field_may_be_shadowed() {
        let src = r#"class A {
            String value;
            void m(java.util.List<String> xs) { xs.forEach(value -> use(value)); }
        }"#;
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    /// A type body opens a new scope: a lambda inside an anonymous class does not see the method
    /// the anonymous class is written in.
    #[test]
    fn a_type_body_between_them_ends_the_scope() {
        let src = r#"class A {
            void m(String s) {
                Runnable r = new Runnable() {
                    public void run() { java.util.List.of().forEach(s -> use(s)); }
                };
            }
        }"#;
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    /// A typed lambda parameter is read the same way as an inferred one.
    #[test]
    fn a_typed_lambda_parameter_is_seen_too() {
        let src = r#"class A {
            void m(String s, java.util.List<String> xs) { xs.forEach((String s) -> use(s)); }
        }"#;
        assert!(msgs(src).iter().any(|m| m.contains("`s` is already defined")), "{:?}", msgs(src));
    }

    /// A catch parameter and an enhanced-for variable are in scope for what is inside them.
    #[test]
    fn a_catch_parameter_and_a_for_variable_are_in_scope() {
        let caught = r#"class A {
            void m(java.util.List<String> xs) { try { } catch (Exception e) { xs.forEach(e -> use(e)); } }
        }"#;
        assert!(msgs(caught).iter().any(|m| m.contains("`e` is already defined")), "{:?}", msgs(caught));
        let looped = r#"class A {
            void m(java.util.List<String> xs) { for (String q : xs) { xs.forEach(q -> use(q)); } }
        }"#;
        assert!(msgs(looped).iter().any(|m| m.contains("`q` is already defined")), "{:?}", msgs(looped));
    }
}
