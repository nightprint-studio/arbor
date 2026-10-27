//! Unrelated default methods (pure-AST, JLS §8.4.8.4): a class that implements two interfaces which each
//! provide a `default` method with the same signature — and that does not override it — does not
//! compile, because nothing says which one wins.
//!
//! Only the shape that is certain from this file alone is judged:
//!   * both interfaces are declared in the file, by a simple name nothing else in it shares;
//!   * neither extends anything (a common ancestor could make one default override the other) and
//!     neither is generic (the signatures would have to be compared after substitution);
//!   * the class has no `extends` (a superclass method would win over both defaults) and declares no
//!     method of that name at all, of any arity.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::support::nodes::child_of_kind;

const TYPE_KINDS: [&str; 5] =
    ["class_declaration", "interface_declaration", "enum_declaration", "record_declaration", "annotation_type_declaration"];

/// An interface the check can reason about: its name and the `default` methods it declares as
/// `(name, parameter types as written)`.
struct Interface {
    defaults: Vec<(String, String)>,
}

/// All unrelated-default conflicts over the shared pre-collected node list.
pub fn default_conflict_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut name_count: HashMap<&str, usize> = HashMap::new();
    for &n in nodes.iter().filter(|n| TYPE_KINDS.contains(&n.kind())) {
        if let Some(name) = declared_name(n, bytes) {
            *name_count.entry(name).or_default() += 1;
        }
    }
    let interfaces: HashMap<&str, Interface> = nodes
        .iter()
        .filter(|n| n.kind() == "interface_declaration")
        .filter_map(|&n| {
            let name = declared_name(n, bytes)?;
            (name_count.get(name) == Some(&1)).then_some(())?;
            Some((name, readable_interface(n, bytes)?))
        })
        .collect();

    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "class_declaration") {
        check_class(n, bytes, &interfaces, &mut out);
    }
    out
}

fn declared_name<'s>(decl: Node, bytes: &'s [u8]) -> Option<&'s str> {
    decl.child_by_field_name("name")?.utf8_text(bytes).ok()
}

/// The interface's defaults, or `None` when it extends something or is generic.
fn readable_interface(decl: Node, bytes: &[u8]) -> Option<Interface> {
    if child_of_kind(decl, "extends_interfaces").is_some() || decl.child_by_field_name("type_parameters").is_some() {
        return None;
    }
    let body = decl.child_by_field_name("body")?;
    let mut c = body.walk();
    let defaults = body
        .named_children(&mut c)
        .filter(|m| m.kind() == "method_declaration" && is_default_method(*m))
        .filter_map(|m| Some((declared_name(m, bytes)?.to_string(), parameter_types(m, bytes)?)))
        .collect();
    Some(Interface { defaults })
}

fn is_default_method(method: Node) -> bool {
    child_of_kind(method, "modifiers").is_some_and(|mods| {
        let mut c = mods.walk();
        let found = mods.children(&mut c).any(|m| m.kind() == "default");
        found
    })
}

/// Parameter types as written, whitespace removed and comma-joined (`int,String`).
fn parameter_types(method: Node, bytes: &[u8]) -> Option<String> {
    let params = method.child_by_field_name("parameters")?;
    let mut c = params.walk();
    let types: Option<Vec<String>> = params
        .named_children(&mut c)
        .map(|p| {
            let ty = p.child_by_field_name("type")?.utf8_text(bytes).ok()?;
            Some(ty.split_whitespace().collect::<String>())
        })
        .collect();
    Some(types?.join(","))
}

fn check_class(class: Node, bytes: &[u8], interfaces: &HashMap<&str, Interface>, out: &mut Vec<Diagnostic>) {
    if class.child_by_field_name("superclass").is_some() {
        return;
    }
    let Some(listed) = class.child_by_field_name("interfaces") else { return };
    let Some(names) = simple_type_names(listed, bytes) else { return };
    let Some(body) = class.child_by_field_name("body") else { return };
    let own_methods: Vec<&str> = {
        let mut c = body.walk();
        let found = body
            .named_children(&mut c)
            .filter(|m| m.kind() == "method_declaration")
            .filter_map(|m| declared_name(m, bytes))
            .collect();
        found
    };
    let class_name = declared_name(class, bytes).unwrap_or("this class");
    let anchor = class.child_by_field_name("name").unwrap_or(class);
    for (i, first) in names.iter().enumerate() {
        for second in &names[i + 1..] {
            let (Some(a), Some(b)) = (interfaces.get(first), interfaces.get(second)) else { continue };
            if first == second {
                continue;
            }
            for (name, params) in &a.defaults {
                if own_methods.contains(&name.as_str()) || !b.defaults.contains(&(name.clone(), params.clone())) {
                    continue;
                }
                out.push(crate::engine::check_id::CheckId::IncompatibleType.at(
                    anchor,
                    format!(
                        "Class `{class_name}` inherits the default method `{name}({params})` from both `{first}` and `{second}` \
                         — override it to say which one wins"
                    ),
                ));
            }
        }
    }
}

/// The names in an `implements` list when every entry is a plain simple name (no generics, no
/// qualification); `None` otherwise, since the rest cannot be matched against this file's interfaces.
fn simple_type_names<'s>(super_interfaces: Node, bytes: &'s [u8]) -> Option<Vec<&'s str>> {
    let list = child_of_kind(super_interfaces, "type_list")?;
    let mut c = list.walk();
    let names = list
        .named_children(&mut c)
        .map(|t| (t.kind() == "type_identifier").then(|| t.utf8_text(bytes).ok()).flatten())
        .collect();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        default_conflict_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    const TWO: &str = "interface L { default String name() { return \"l\"; } } interface R { default String name() { return \"r\"; } }";

    #[test]
    fn unrelated_defaults_without_an_override_are_flagged() {
        let m = errors(&format!("{TWO} class C implements L, R {{ }}"));
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("`name()`") && m[0].contains("`L`") && m[0].contains("`R`"), "{m:?}");
    }

    #[test]
    fn an_override_a_superclass_or_a_shared_ancestor_clears_it() {
        assert!(errors(&format!("{TWO} class C implements L, R {{ public String name() {{ return \"\"; }} }}")).is_empty());
        assert!(errors(&format!("{TWO} class C extends B implements L, R {{ }}")).is_empty());
        // R extends L: its default overrides L's, so nothing is ambiguous.
        assert!(errors("interface L { default int n() { return 1; } } interface R extends L { default int n() { return 2; } } class C implements L, R { }").is_empty());
    }

    #[test]
    fn different_signatures_and_abstract_methods_are_not_conflicts() {
        assert!(errors("interface L { default int n(int a) { return 1; } } interface R { default int n(long a) { return 2; } } class C implements L, R { }").is_empty());
        assert!(errors("interface L { default int n() { return 1; } } interface R { int n(); } class C implements L, R { public int n() { return 0; } }").is_empty());
    }
}
