//! `@Target({ElementType.TYPE, ElementType.TYPE})` — the same element kind listed twice
//! (`repeated.annotation.target`). javac refuses it outright rather than treating the list as a set.
//!
//! Decided from the written names alone: two entries with the same last segment (`TYPE`,
//! `ElementType.TYPE` and `java.lang.annotation.ElementType.TYPE` are one kind). A file that declares
//! its own `Target` annotation is skipped — then the name is not `java.lang.annotation.Target`.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::text;

/// The second occurrence of each element kind in every `@Target` list.
pub fn target_dupe_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let own_target = nodes.iter().any(|n| {
        matches!(n.kind(), "annotation_type_declaration" | "class_declaration" | "interface_declaration")
            && n.child_by_field_name("name").and_then(|x| text(x, bytes)).is_some_and(|t| t == "Target")
    });
    if own_target {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "annotation" {
            continue;
        }
        let is_target = n
            .child_by_field_name("name")
            .and_then(|x| text(x, bytes))
            .is_some_and(|t| t == "Target" || t == "java.lang.annotation.Target");
        let Some(value) = n.child_by_field_name("arguments").filter(|_| is_target) else { continue };
        let Some(list) = first_array(value) else { continue };
        let mut seen = std::collections::HashSet::new();
        let mut c = list.walk();
        for element in list.named_children(&mut c) {
            let Some(written) = text(element, bytes) else { continue };
            let kind = written.rsplit('.').next().unwrap_or(&written).trim().to_string();
            if !seen.insert(kind.clone()) {
                out.push(CheckId::AnnotationNotApplicable.at(element, format!("`{kind}` is listed twice in `@Target`")));
            }
        }
    }
    out
}

/// The array initializer among an annotation's arguments, however the value is spelled
/// (`@Target({…})` or `@Target(value = {…})`).
fn first_array(arguments: Node) -> Option<Node> {
    let mut stack = vec![arguments];
    while let Some(n) = stack.pop() {
        if n.kind() == "element_value_array_initializer" {
            return Some(n);
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        target_dupe_errors_nodes(&nodes, src).len()
    }

    #[test]
    fn a_repeated_element_kind_is_reported_once() {
        assert_eq!(count("@Target({ElementType.TYPE, ElementType.TYPE}) @interface A { }"), 1);
        assert_eq!(count("@Target({TYPE, ElementType.TYPE, METHOD}) @interface A { }"), 1);
        assert_eq!(count("@Target(value = {METHOD, METHOD}) @interface A { }"), 1);
    }

    #[test]
    fn distinct_kinds_and_other_annotations_are_left_alone() {
        assert_eq!(count("@Target({ElementType.TYPE, ElementType.METHOD}) @interface A { }"), 0);
        assert_eq!(count("@Retention({RUNTIME, RUNTIME}) @interface A { }"), 0);
        assert_eq!(count("@interface Target { } @Target({X, X}) @interface A { }"), 0);
    }
}
