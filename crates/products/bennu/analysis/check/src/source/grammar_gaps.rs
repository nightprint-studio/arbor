//! Shapes tree-sitter reads without complaint and Java rejects.
//!
//! The grammar is deliberately forgiving — it exists to highlight and navigate half-typed code — so a
//! handful of things javac's parser refuses come out of it as ordinary, error-free nodes. Each one
//! below is decided from the shape of the tree alone, with the text javac reports it under:
//!
//! * `assert` / `enum` used as a name (`assert.as.identifier`, `enum.as.identifier`) — both are keywords;
//! * an access modifier or `static` on a local variable (`illegal.start.of.expr`);
//! * a declaration as the body of an `if` / loop, with no block around it (`variable.not.allowed`,
//!   `class.not.allowed`);
//! * a method with no return type whose name is not its class's (`invalid.meth.decl.ret.type.req`);
//! * a varargs parameter that is not the last one (`varargs.must.be.last`);
//! * a field, method or statement outside any type (`class, interface, enum, or record expected`).
//!
//! All of them are errors at every language level, so none consults the project's Java version.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::{modifier_keywords, text};

/// Every grammar-gap error in the pre-collected node slice.
pub fn grammar_gap_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "identifier" => keyword_as_name(n, bytes, &mut out),
            "local_variable_declaration" => {
                local_modifier(n, bytes, &mut out);
                stray_at_top_level(n, &mut out);
            }
            "field_declaration" | "method_declaration" => stray_at_top_level(n, &mut out),
            "if_statement" => {
                for field in ["consequence", "alternative"] {
                    if let Some(body) = n.child_by_field_name(field) {
                        declaration_as_body(body, &mut out);
                    }
                }
            }
            "while_statement" | "do_statement" | "for_statement" | "enhanced_for_statement" => {
                if let Some(body) = n.child_by_field_name("body") {
                    declaration_as_body(body, &mut out);
                }
            }
            "constructor_declaration" => constructor_named_wrongly(n, bytes, &mut out),
            "formal_parameters" => varargs_not_last(n, &mut out),
            _ => {}
        }
    }
    out
}

fn keyword_as_name(id: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(name) = text(id, bytes) else { return };
    if name == "assert" || name == "enum" {
        out.push(CheckId::SyntaxError.at(id, format!("`{name}` is a keyword and cannot be used as an identifier")));
    }
}

fn local_modifier(local: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let illegal = modifier_keywords(local, bytes)
        .into_iter()
        .find(|m| matches!(*m, "public" | "protected" | "private" | "static"));
    if let Some(modifier) = illegal {
        out.push(CheckId::SyntaxError.at(local, format!("Illegal start of expression: `{modifier}` on a local variable")));
    }
}

/// A member or statement written directly in the file, outside any type.
fn stray_at_top_level(n: Node, out: &mut Vec<Diagnostic>) {
    if n.parent().is_some_and(|p| p.kind() == "program") {
        out.push(CheckId::SyntaxError.at(n, "Class, interface, enum, or record expected"));
    }
}

fn declaration_as_body(body: Node, out: &mut Vec<Diagnostic>) {
    match body.kind() {
        "local_variable_declaration" => {
            out.push(CheckId::IllegalDeclaration.at(body, "A variable declaration is not allowed here: it needs a block"));
        }
        "class_declaration" => {
            out.push(CheckId::IllegalDeclaration.at(body, "A class declaration is not allowed here: it needs a block"));
        }
        _ => {}
    }
}

/// `compute() { }` in a class that is not called `compute`: a method that forgot its return type.
fn constructor_named_wrongly(ctor: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(name_node) = ctor.child_by_field_name("name") else { return };
    let Some(name) = text(name_node, bytes) else { return };
    let owner = ctor
        .parent()
        .filter(|p| p.kind() == "class_body")
        .and_then(|b| b.parent())
        .filter(|o| matches!(o.kind(), "class_declaration" | "enum_declaration" | "record_declaration"))
        .and_then(|o| o.child_by_field_name("name"))
        .and_then(|n| text(n, bytes));
    if let Some(owner) = owner {
        if owner != name {
            out.push(CheckId::IllegalDeclaration.at(name_node, "Invalid method declaration: a return type is required"));
        }
    }
}

fn varargs_not_last(params: Node, out: &mut Vec<Diagnostic>) {
    let mut c = params.walk();
    let list: Vec<Node> = params.named_children(&mut c).collect();
    for (i, p) in list.iter().enumerate() {
        if p.kind() == "spread_parameter" && i + 1 < list.len() {
            out.push(CheckId::IllegalDeclaration.at(*p, "A varargs parameter must be the last parameter"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        grammar_gap_errors_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn keywords_used_as_names_are_reported() {
        assert_eq!(run("class A { void m() { int assert = 1; } }"), ["syntax-error"]);
        assert_eq!(run("class A { int enum = 1; }"), ["syntax-error"]);
        assert!(run("class A { void m(int a) { assert a > 0; } }").is_empty());
    }

    #[test]
    fn an_access_modifier_on_a_local_is_reported_and_final_is_not() {
        assert_eq!(run("class A { void m() { public int x = 1; } }"), ["syntax-error"]);
        assert_eq!(run("class A { void m() { static int x = 1; } }"), ["syntax-error"]);
        assert!(run("class A { void m() { final int x = 1; } }").is_empty());
    }

    #[test]
    fn a_declaration_needs_a_block_under_an_if_or_a_loop() {
        assert_eq!(run("class A { void m(boolean f) { if (f) int x = 1; } }"), ["illegal-declaration"]);
        assert_eq!(run("class A { void m(boolean f) { while (f) class L { } } }"), ["illegal-declaration"]);
        assert!(run("class A { void m(boolean f) { if (f) { int x = 1; } else foo(); } }").is_empty());
    }

    #[test]
    fn a_method_without_a_return_type_is_reported_unless_it_is_a_constructor() {
        assert_eq!(run("class A { compute() { } }"), ["illegal-declaration"]);
        assert!(run("class A { A() { } }").is_empty());
        assert!(run("enum E { ONE; E() { } }").is_empty());
        assert!(run("record R(int a) { R { } }").is_empty());
    }

    #[test]
    fn varargs_must_come_last() {
        assert_eq!(run("class A { void m(int... v, int last) { } }"), ["illegal-declaration"]);
        assert!(run("class A { void m(int first, int... v) { } }").is_empty());
    }

    #[test]
    fn nothing_may_stand_outside_a_type() {
        assert_eq!(run("class A { } int stray;"), ["syntax-error"]);
        assert!(run("package p; import java.util.List; class A { int ok; }").is_empty());
    }
}
