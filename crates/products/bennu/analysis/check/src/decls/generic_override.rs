//! `name.clash.same.erasure.no.override` for a subclass of a generic class declared in the same file:
//!
//! ```java
//! class Base<T> { void accept(T value) { } }
//! class Narrowed extends Base<String> { void accept(Object value) { } }   // clash
//! ```
//!
//! `Base<String>` inherits `accept(String)`, so `accept(Object)` does not override it — yet the two
//! have the same erasure, `accept(Object)`, and a class cannot hold both. The subclass method is
//! neither an override nor a legal overload.
//!
//! Kept to the shape that can be decided from the tree with no doubt: both classes in this file, the
//! base's parameter is a bare **unbounded** type variable (its erasure is `Object`) or a plain
//! non-generic type, the subclass names the base with concrete type arguments, and the subclass
//! parameter erases to the same thing while the argument the variable stands for does not. A bounded
//! variable, a generic parameter type, a base declared elsewhere: all skipped.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::text;

/// Every subclass method that clashes with a generic base method it does not override.
pub fn generic_override_clash_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let classes: HashMap<String, Node> = nodes
        .iter()
        .filter(|n| n.kind() == "class_declaration")
        .filter_map(|n| Some((n.child_by_field_name("name").and_then(|x| text(x, bytes))?, *n)))
        .collect();
    let mut out = Vec::new();
    for class in classes.values() {
        let Some((base_name, arguments)) = extended_generic(*class, bytes) else { continue };
        let Some(base) = classes.get(&base_name) else { continue };
        let Some(variables) = type_variables(*base, bytes) else { continue };
        if variables.len() != arguments.len() {
            continue;
        }
        let (Some(body), Some(base_body)) = (class.child_by_field_name("body"), base.child_by_field_name("body")) else {
            continue;
        };
        for sub in methods(body) {
            for sup in methods(base_body) {
                if clashes(sub, sup, &variables, &arguments, bytes) {
                    if let Some(name) = sub.child_by_field_name("name") {
                        out.push(CheckId::ErasureClash.at(
                            name,
                            "Name clash: this method has the same erasure as a method of the generic base, yet does not override it",
                        ));
                    }
                }
            }
        }
    }
    out
}

/// `extends Base<A, B>` → `("Base", ["A", "B"])`; `None` when the superclass is not parameterized.
fn extended_generic(class: Node, bytes: &[u8]) -> Option<(String, Vec<String>)> {
    let superclass = class.child_by_field_name("superclass")?;
    let mut c = superclass.walk();
    let generic = superclass.named_children(&mut c).find(|n| n.kind() == "generic_type")?;
    let mut gc = generic.walk();
    let name = generic.named_children(&mut gc).find(|n| n.kind() == "type_identifier").and_then(|n| text(n, bytes))?;
    let args = generic.named_children(&mut gc).find(|n| n.kind() == "type_arguments")?;
    let mut ac = args.walk();
    let arguments: Vec<String> = args.named_children(&mut ac).filter_map(|n| text(n, bytes)).collect();
    // A wildcard or a nested generic argument is outside what this decides.
    if arguments.iter().any(|a| a.contains('?') || a.contains('<')) {
        return None;
    }
    Some((name, arguments))
}

/// The base's type-variable names, in order — `None` if any is bounded (its erasure is then not `Object`).
fn type_variables(class: Node, bytes: &[u8]) -> Option<Vec<String>> {
    let params = class.child_by_field_name("type_parameters")?;
    let mut c = params.walk();
    let mut out = Vec::new();
    for tp in params.named_children(&mut c).filter(|n| n.kind() == "type_parameter") {
        let mut tc = tp.walk();
        let parts: Vec<Node> = tp.named_children(&mut tc).collect();
        if parts.len() != 1 {
            return None; // `T extends X` carries a bound
        }
        out.push(text(parts[0], bytes)?);
    }
    Some(out)
}

fn methods(body: Node) -> Vec<Node> {
    let mut c = body.walk();
    body.named_children(&mut c).filter(|m| m.kind() == "method_declaration").collect()
}

fn parameter_types(method: Node, bytes: &[u8]) -> Option<Vec<String>> {
    let params = method.child_by_field_name("parameters")?;
    let mut c = params.walk();
    let mut out = Vec::new();
    for p in params.named_children(&mut c) {
        if p.kind() != "formal_parameter" {
            return None; // varargs: not judged
        }
        out.push(text(p.child_by_field_name("type")?, bytes)?);
    }
    Some(out)
}

fn is_static_or_private(method: Node, bytes: &[u8]) -> bool {
    let mut c = method.walk();
    let modifiers: Vec<Node> = method.children(&mut c).filter(|m| m.kind() == "modifiers").collect();
    modifiers.into_iter().any(|m| {
        text(m, bytes).is_some_and(|t| t.split_whitespace().any(|w| w == "static" || w == "private"))
    })
}

fn clashes(sub: Node, sup: Node, variables: &[String], arguments: &[String], bytes: &[u8]) -> bool {
    let same_name = sub.child_by_field_name("name").and_then(|n| text(n, bytes))
        == sup.child_by_field_name("name").and_then(|n| text(n, bytes));
    if !same_name || is_static_or_private(sub, bytes) || is_static_or_private(sup, bytes) {
        return false;
    }
    let (Some(own), Some(inherited)) = (parameter_types(sub, bytes), parameter_types(sup, bytes)) else {
        return false;
    };
    if own.len() != inherited.len() || own.is_empty() {
        return false;
    }
    let mut differs_after_substitution = false;
    for (mine, theirs) in own.iter().zip(&inherited) {
        if theirs.contains('<') || mine.contains('<') {
            return false;
        }
        match variables.iter().position(|v| v == theirs) {
            Some(at) => {
                // The variable erases to `Object`; the subclass parameter must too.
                if mine != "Object" {
                    return false;
                }
                if arguments[at] != "Object" {
                    differs_after_substitution = true;
                }
            }
            None => {
                if mine != theirs {
                    return false;
                }
            }
        }
    }
    differs_after_substitution
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        generic_override_clash_nodes(&nodes, src).len()
    }

    #[test]
    fn an_object_parameter_against_a_variable_bound_to_string_is_a_clash() {
        assert_eq!(count("class B<T> { void a(T v) { } } class N extends B<String> { void a(Object v) { } }"), 1);
    }

    #[test]
    fn a_real_override_an_overload_or_a_bound_is_left_alone() {
        assert_eq!(count("class B<T> { void a(T v) { } } class N extends B<String> { void a(String v) { } }"), 0);
        assert_eq!(count("class B<T> { void a(T v) { } } class N extends B<Object> { void a(Object v) { } }"), 0);
        assert_eq!(count("class B<T> { void a(T v) { } } class N extends B<String> { void a(int v) { } }"), 0);
        assert_eq!(count("class B<T extends Number> { void a(T v) { } } class N extends B<Integer> { void a(Number v) { } }"), 0);
        assert_eq!(count("class B<T> { void a(T v) { } } class N extends B<String> { static void a(Object v) { } }"), 0);
    }
}
