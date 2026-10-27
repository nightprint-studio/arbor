//! What an enum may not do — three rules javac states as separate errors and that need nothing but
//! the declaration to decide:
//!
//! * **`class X extends Enum<X>`** (`enum.no.subclassing`) — only the compiler may extend
//!   `java.lang.Enum`; the `enum` keyword is the way to get one. Stays silent when the file declares
//!   or imports a different type called `Enum`, because then the name is not `java.lang.Enum`.
//! * **`protected void finalize()` in an enum** (`enum.no.finalize`) — an enum constant is never
//!   finalized, and `Enum` declares `finalize` final.
//! * **A static field read from the enum's constructor or an instance initializer**
//!   (`illegal.enum.static.ref`) — the constants are constructed while the class initializes, before
//!   the static fields are, so the field would be read in its default state. A constant variable
//!   (`static final` with a literal) is exempt: it is inlined, not read.
//!
//! The third is the only one with judgement in it, and it is kept narrow on purpose: a bare name that
//! is not redeclared in the constructor, naming a non-constant static field of the same enum.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::{has_keyword, text};

/// Every enum-rule violation in the pre-collected node slice.
pub fn enum_rule_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let custom_enum = declares_or_imports_other_enum(nodes, bytes);
    for &n in nodes {
        match n.kind() {
            "class_declaration" if !custom_enum => extends_enum(n, bytes, &mut out),
            "enum_declaration" => {
                if let Some(body) = n.child_by_field_name("body") {
                    finalize_method(body, bytes, &mut out);
                    static_references(body, bytes, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// A type named `Enum` that is not `java.lang.Enum`: declared here, or imported by a single import.
fn declares_or_imports_other_enum(nodes: &[Node], bytes: &[u8]) -> bool {
    nodes.iter().any(|n| match n.kind() {
        "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration" => {
            n.child_by_field_name("name").and_then(|x| text(x, bytes)).is_some_and(|t| t == "Enum")
        }
        "import_declaration" => n
            .utf8_text(bytes)
            .is_ok_and(|t| t.trim_end_matches(';').trim_end().ends_with(".Enum") && !t.contains("java.lang.Enum")),
        _ => false,
    })
}

fn extends_enum(class: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(superclass) = class.child_by_field_name("superclass") else { return };
    let Ok(written) = superclass.utf8_text(bytes) else { return };
    let name = written.trim_start_matches("extends").trim();
    let erased = name.split('<').next().unwrap_or(name).trim();
    if erased == "Enum" || erased == "java.lang.Enum" {
        out.push(CheckId::IllegalInheritance.at(superclass, "Classes cannot directly extend `java.lang.Enum`: declare an `enum`"));
    }
}

fn finalize_method(body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    for member in members(body) {
        if member.kind() != "method_declaration" {
            continue;
        }
        let named = member.child_by_field_name("name").and_then(|n| text(n, bytes));
        let no_params = member
            .child_by_field_name("parameters")
            .is_some_and(|p| p.named_child_count() == 0);
        if named.as_deref() == Some("finalize") && no_params {
            if let Some(name) = member.child_by_field_name("name") {
                out.push(CheckId::IllegalDeclaration.at(name, "An enum cannot declare `finalize()`"));
            }
        }
    }
}

/// The members of an enum body, whether they sit directly in it or under `enum_body_declarations`.
fn members(body: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut c = body.walk();
    for child in body.named_children(&mut c) {
        if child.kind() == "enum_body_declarations" {
            let mut cc = child.walk();
            out.extend(child.named_children(&mut cc));
        } else {
            out.push(child);
        }
    }
    out
}

fn static_references(body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let members = members(body);
    let statics: HashSet<String> = members
        .iter()
        .filter(|m| m.kind() == "field_declaration" && has_keyword(**m, bytes, "static"))
        .flat_map(|m| non_constant_fields(*m, bytes))
        .collect();
    if statics.is_empty() {
        return;
    }
    for member in &members {
        let scope = match member.kind() {
            "constructor_declaration" => member.child_by_field_name("body"),
            "block" => Some(*member), // an instance initializer
            _ => None,
        };
        let Some(scope) = scope else { continue };
        let own = declared_names(*member, bytes);
        let mut stack = vec![scope];
        while let Some(n) = stack.pop() {
            if matches!(n.kind(), "class_body" | "lambda_expression") {
                continue; // an anonymous class or lambda has its own idea of `this` and of timing
            }
            if n.kind() == "identifier" && is_bare_read_or_write(n) {
                if let Some(name) = text(n, bytes) {
                    if statics.contains(&name) && !own.contains(&name) {
                        out.push(CheckId::StaticContextAccess.at(
                            n,
                            format!("Illegal reference to the static field `{name}` from an enum constructor or initializer"),
                        ));
                    }
                }
            }
            let mut c = n.walk();
            stack.extend(n.children(&mut c));
        }
    }
}

/// The names a field declaration introduces that are NOT constant variables.
pub(crate) fn non_constant_fields(field: Node, bytes: &[u8]) -> Vec<String> {
    let is_final = has_keyword(field, bytes, "final");
    let mut out = Vec::new();
    let mut c = field.walk();
    for d in field.children_by_field_name("declarator", &mut c) {
        let Some(name) = d.child_by_field_name("name").and_then(|n| text(n, bytes)) else { continue };
        let value = d.child_by_field_name("value");
        let constant = is_final && value.is_none_or(|v| !is_computed(v));
        if !constant {
            out.push(name);
        }
    }
    out
}

/// An initializer that is surely computed at run time, so the field is not a constant variable.
fn is_computed(value: Node) -> bool {
    let mut stack = vec![value];
    while let Some(n) = stack.pop() {
        if matches!(
            n.kind(),
            "method_invocation" | "object_creation_expression" | "array_creation_expression" | "lambda_expression"
        ) {
            return true;
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c));
    }
    false
}

/// Every name the constructor or initializer declares itself — parameters, locals, catch and loop
/// variables — so a bare name that one of them hides is never mistaken for the static field.
pub(crate) fn declared_names(scope: Node, bytes: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        if matches!(
            n.kind(),
            "formal_parameter" | "variable_declarator" | "catch_formal_parameter" | "enhanced_for_statement" | "resource"
        ) {
            if let Some(name) = n.child_by_field_name("name").and_then(|x| text(x, bytes)) {
                names.insert(name);
            }
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c));
    }
    names
}

/// A bare identifier used as a value: not the field of `a.b`, not the name of a call, not a declared
/// name.
pub(crate) fn is_bare_read_or_write(id: Node) -> bool {
    let Some(parent) = id.parent() else { return false };
    match parent.kind() {
        "field_access" => parent.child_by_field_name("object").is_some_and(|o| o.id() == id.id()),
        "method_invocation" => parent.child_by_field_name("name").is_none_or(|n| n.id() != id.id()),
        // `int x = N;` — `N` is the value, `x` the name.
        "variable_declarator" => parent.child_by_field_name("value").is_some_and(|v| v.id() == id.id()),
        "formal_parameter" | "scoped_identifier" | "scoped_type_identifier" | "labeled_statement"
        | "break_statement" | "continue_statement" | "marker_annotation" | "annotation" | "method_reference" => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        enum_rule_errors_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn extending_enum_directly_is_reported_unless_enum_is_another_type() {
        assert_eq!(run("class X extends Enum<X> { }"), ["illegal-inheritance"]);
        assert!(run("class Enum { } class X extends Enum { }").is_empty());
        assert!(run("import acme.Enum; class X extends Enum { }").is_empty());
    }

    #[test]
    fn finalize_in_an_enum_is_reported() {
        assert_eq!(run("enum E { ONE; protected void finalize() { } }"), ["illegal-declaration"]);
        assert!(run("enum E { ONE; void finalize(int x) { } }").is_empty());
    }

    #[test]
    fn a_static_field_read_from_the_constructor_is_reported() {
        let bad = "enum E { ONE; static int counter; E() { counter++; } }";
        assert_eq!(run(bad), ["static-context-access"]);
    }

    #[test]
    fn constants_other_names_and_qualified_reads_are_left_alone() {
        assert!(run("enum E { ONE; static final int N = 2; E() { int x = N; } }").is_empty());
        assert!(run("enum E { ONE; static int counter; E(int counter) { counter++; } }").is_empty());
        assert!(run("enum E { ONE; static int counter; E() { this.counter = 1; } void m() { counter++; } }").is_empty());
        assert!(run("enum E { ONE; static final int[] A = new int[1]; E() { int x = A.length; } }")
            .contains(&"static-context-access".to_string()));
    }
}
