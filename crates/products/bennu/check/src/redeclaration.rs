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
use tree_sitter::{Node, Parser};

/// Parse `source` and flag illegal redeclarations.
pub fn redeclaration_errors(source: &str) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    match parser.parse(source, None) {
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
        severity: "error".to_string(),
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
