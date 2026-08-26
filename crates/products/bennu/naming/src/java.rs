//! The Java pack.
//!
//! ## What it deliberately does not report
//!
//! **Constructors.** A constructor's name is the class's name; "fixing" it means renaming the
//! type. Reporting it would put a second, misleading squiggle on a violation that is already
//! reported once, on the type.
//!
//! **`@Override` methods.** The name is not the author's to choose — it is the supertype's, and
//! very often the JDK's or a framework's. A diagnostic whose only honest fix is "rename something
//! you do not own" is noise, and on a Struts/Spring tree it would be most of the output.
//!
//! **JDK-mandated names.** `serialVersionUID` is spelled that way because `java.io.Serializable`
//! says so. Under `UPPER_SNAKE_CASE` it would otherwise be flagged in every serialisable class in
//! the project, and the fix would break serialisation.
//!
//! Interface fields arrive as `constant_declaration` from the grammar, so they are constants
//! without anyone having to look at the enclosing type; a class field is a constant when it is
//! `static final`.

use tree_sitter::{Language, Node};

use crate::convention::Convention::{Camel, Lower, Pascal, UpperSnake};
use crate::pack::{push_named, push_node, text, DeclSource, Declared, GrammarWalk, Pack};
use crate::target::Target;

/// Names the platform fixes, which a convention must never rewrite.
const JDK_MANDATED: [&str; 2] = ["serialVersionUID", "serialPersistentFields"];

/// The only pack whose declarations come from a grammar rather than a language server — because
/// Bennu's Java engine is its own, and there is no server to ask. It is also the only pack with no
/// blind spots: locals, parameters, type parameters and package segments are all in the tree.
pub const JAVA: Pack = Pack {
    id: "java",
    label: "Java",
    extensions: &["java"],
    standard: &[
        (Target::Type, Pascal),
        (Target::Method, Camel),
        (Target::Field, Camel),
        (Target::Constant, UpperSnake),
        (Target::Parameter, Camel),
        (Target::Local, Camel),
        (Target::TypeParameter, Pascal),
        (Target::EnumConstant, UpperSnake),
        (Target::Package, Lower),
    ],
    source: DeclSource::Grammar(&JavaPack),
};

pub struct JavaPack;

impl GrammarWalk for JavaPack {
    fn language(&self) -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn declarations(&self, node: Node, source: &str, out: &mut Vec<Declared>) {
        match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => push_named(node, source, Target::Type, out),

            // A constructor is named by its class — see the module doc.
            "method_declaration" if !has_override(node, source) => {
                push_named(node, source, Target::Method, out)
            }
            // `int value();` in an `@interface` reads as a method and is spelled like one.
            "annotation_type_element_declaration" => push_named(node, source, Target::Method, out),

            // `static final` makes a class field a constant; anything else is a field.
            "field_declaration" => {
                let target =
                    if is_static_final(node, source) { Target::Constant } else { Target::Field };
                push_declarators(node, source, target, out);
            }
            // The grammar's own node for an interface field, which is implicitly `static final`.
            "constant_declaration" => push_declarators(node, source, Target::Constant, out),

            "local_variable_declaration" => push_declarators(node, source, Target::Local, out),
            // `for (String x : xs)` and `try (Reader r = …)` declare a local outside any declarator.
            "enhanced_for_statement" | "resource" => push_named(node, source, Target::Local, out),

            // A record's components arrive as `formal_parameter`s, and they are **not** parameters:
            // each one is a private final field plus a generated accessor, and the code calls
            // `failure.source_path()`. Classifying them as a parameter makes them file-local, which
            // means the fix would rename the component on the spot and leave every accessor call
            // behind — a rename that reports success and stops the project compiling.
            "formal_parameter" if is_record_component(node) => {
                push_named(node, source, Target::Field, out)
            }
            "formal_parameter" | "catch_formal_parameter" => {
                push_named(node, source, Target::Parameter, out)
            }
            "spread_parameter" => push_declarators(node, source, Target::Parameter, out),
            // `(a, b) -> …`: untyped lambda parameters are bare identifiers.
            "inferred_parameters" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "identifier" {
                        push_node(child, source, Target::Parameter, out);
                    }
                }
            }
            // `x -> …`: a SINGLE untyped parameter has no parentheses, so the grammar gives a bare
            // `identifier` in the `parameters` field rather than an `inferred_parameters` node.
            // Matching only the parenthesised form left the most common lambda in Java unchecked.
            "lambda_expression" => {
                if let Some(params) = node.child_by_field_name("parameters") {
                    if params.kind() == "identifier" {
                        push_node(params, source, Target::Parameter, out);
                    }
                }
            }

            "type_parameter" => {
                // Bound before the `if let`: the iterator borrows `cursor`, and as the scrutinee of
                // an `if let` it lives to the end of the block — past `cursor` itself.
                let mut cursor = node.walk();
                let name = node.named_children(&mut cursor).find(|c| c.kind() == "type_identifier");
                if let Some(name) = name {
                    push_node(name, source, Target::TypeParameter, out);
                }
            }

            "enum_constant" => push_named(node, source, Target::EnumConstant, out),

            // Every segment of `package com.acme.legacy;` is checked on its own, so the report
            // points at the segment that is wrong rather than at the whole path.
            "package_declaration" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    push_identifiers(child, source, Target::Package, out);
                }
            }

            _ => {}
        }
    }
}

/// Push the `name` of every `variable_declarator` under `node` (a `int a, b;` declares two).
///
/// This is also where the platform's own names are dropped: `serialVersionUID` reaches the pack
/// as a field declarator and nowhere else, so filtering here costs one comparison per declaration
/// instead of a re-scan of everything found so far.
fn push_declarators(node: Node, source: &str, target: Target, out: &mut Vec<Declared>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let named = child.child_by_field_name("name");
        if named.map(|n| text(n, source)).is_some_and(|n| JDK_MANDATED.contains(&n)) {
            continue;
        }
        push_named(child, source, target, out);
    }
}

/// Push every `identifier` in the subtree — how a dotted package path yields one entry per segment.
fn push_identifiers(node: Node, source: &str, target: Target, out: &mut Vec<Declared>) {
    if node.kind() == "identifier" {
        push_node(node, source, target, out);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        push_identifiers(child, source, target, out);
    }
}

/// Whether this `formal_parameter` is a **record component** — `record Foo(int a_b)` — rather than
/// a method parameter. The grammar spells both the same way; only the grandparent tells them apart.
fn is_record_component(node: Node) -> bool {
    node.parent()
        .and_then(|params| params.parent())
        .map(|owner| owner.kind() == "record_declaration")
        .unwrap_or(false)
}

/// Whether a declaration's `modifiers` carry both `static` and `final`.
fn is_static_final(node: Node, source: &str) -> bool {
    let Some(modifiers) = child_of_kind(node, "modifiers") else { return false };
    let mut cursor = modifiers.walk();
    let words: Vec<&str> =
        modifiers.children(&mut cursor).map(|m| text(m, source)).collect();
    words.contains(&"static") && words.contains(&"final")
}

/// Whether a method carries `@Override`.
fn has_override(node: Node, source: &str) -> bool {
    let Some(modifiers) = child_of_kind(node, "modifiers") else { return false };
    let mut cursor = modifiers.walk();
    // Bound rather than returned directly: as a tail expression the iterator's temporary outlives
    // `cursor`, which the borrow checker rejects.
    let found = modifiers.named_children(&mut cursor).any(|m| {
        matches!(m.kind(), "marker_annotation" | "annotation")
            && m.child_by_field_name("name").map(|n| text(n, source)) == Some("Override")
    });
    found
}

fn child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::declarations_in;

    fn found(source: &str) -> Vec<(Target, String)> {
        declarations_in(&JavaPack, source)
            .into_iter()
            .map(|d| (d.target, d.name))
            .collect()
    }

    fn names_of(source: &str, target: Target) -> Vec<String> {
        found(source).into_iter().filter(|(t, _)| *t == target).map(|(_, n)| n).collect()
    }

    #[test]
    fn classifies_the_common_declarations() {
        let src = r#"
            package com.acme.legacy;
            public class OrderService<T> {
                private static final int MAX_RETRIES = 3;
                private String customerName;
                public void doWork(String input_value) {
                    int local_count = 0;
                }
            }
        "#;
        assert_eq!(names_of(src, Target::Type), ["OrderService"]);
        assert_eq!(names_of(src, Target::Method), ["doWork"]);
        assert_eq!(names_of(src, Target::Field), ["customerName"]);
        assert_eq!(names_of(src, Target::Constant), ["MAX_RETRIES"]);
        assert_eq!(names_of(src, Target::Parameter), ["input_value"]);
        assert_eq!(names_of(src, Target::Local), ["local_count"]);
        assert_eq!(names_of(src, Target::TypeParameter), ["T"]);
        assert_eq!(names_of(src, Target::Package), ["com", "acme", "legacy"]);
    }

    #[test]
    fn one_declaration_per_declarator() {
        let src = "class A { void m() { int a_one, b_two; } }";
        assert_eq!(names_of(src, Target::Local), ["a_one", "b_two"]);
    }

    #[test]
    fn interface_fields_are_constants_without_modifiers() {
        let src = "interface Codes { String DEFAULT_CODE = \"x\"; }";
        assert_eq!(names_of(src, Target::Constant), ["DEFAULT_CODE"]);
        assert!(names_of(src, Target::Field).is_empty());
    }

    #[test]
    fn a_non_final_static_field_is_a_field() {
        let src = "class A { static int counter; }";
        assert_eq!(names_of(src, Target::Field), ["counter"]);
        assert!(names_of(src, Target::Constant).is_empty());
    }

    #[test]
    fn constructors_are_not_methods() {
        let src = "class Order_Item { Order_Item() {} void run() {} }";
        assert_eq!(names_of(src, Target::Method), ["run"]);
        // The type is still reported once, which is where the fix belongs.
        assert_eq!(names_of(src, Target::Type), ["Order_Item"]);
    }

    #[test]
    fn overrides_are_left_alone() {
        let src = "class A { @Override public String to_string() { return null; } void mine() {} }";
        assert_eq!(names_of(src, Target::Method), ["mine"]);
    }

    #[test]
    fn jdk_mandated_names_are_never_reported() {
        let src = "class A implements java.io.Serializable { private static final long serialVersionUID = 1L; }";
        assert!(found(src).iter().all(|(_, n)| n != "serialVersionUID"));
    }

    #[test]
    fn enum_constants_and_enhanced_for_and_catch() {
        let src = r#"
            enum Status { IN_PROGRESS, done_now }
            class A {
                void m(java.util.List<String> xs) {
                    for (String each_one : xs) {}
                    try { } catch (Exception the_error) { }
                }
            }
        "#;
        assert_eq!(names_of(src, Target::EnumConstant), ["IN_PROGRESS", "done_now"]);
        assert_eq!(names_of(src, Target::Local), ["each_one"]);
        assert_eq!(names_of(src, Target::Parameter), ["xs", "the_error"]);
    }

    #[test]
    fn lambda_parameters_are_parameters() {
        let src = "class A { void m() { java.util.function.BiFunction<String,String,String> f = (a_one, b_two) -> a_one; } }";
        assert_eq!(names_of(src, Target::Parameter), ["a_one", "b_two"]);
    }
}
