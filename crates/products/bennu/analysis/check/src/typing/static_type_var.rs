//! A type's own type parameter used where there is no instance to parameterize it (JLS §8.1.3):
//! a static field, method or initializer, or a static nested type. `compiler.err.non-static.cant.be.ref`.
//!
//! Pure AST. A type parameter is in scope by its simple name throughout its declaration, so a name
//! that matches one IS that parameter — unless something nearer declares its own parameter of that
//! name (`static <T> T of(T t)`, `static class Node<T>`), which shadows it, or the file also declares
//! a type of that name, in which case the reading is left alone.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::has_keyword;

const TYPE_DECLS: [&str; 5] =
    ["class_declaration", "interface_declaration", "enum_declaration", "record_declaration", "annotation_type_declaration"];

/// Every use of an enclosing type's type parameter in a static context of that type.
pub fn static_type_var_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut type_names: Option<HashSet<String>> = None;
    for &decl in nodes {
        if !matches!(decl.kind(), "class_declaration" | "interface_declaration" | "record_declaration") {
            continue;
        }
        let own = type_parameter_names(decl, bytes);
        if own.is_empty() {
            continue;
        }
        let names = type_names.get_or_insert_with(|| {
            nodes
                .iter()
                .filter(|n| TYPE_DECLS.contains(&n.kind()))
                .filter_map(|n| n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()))
                .map(str::to_string)
                .collect()
        });
        let params: Vec<String> = own.into_iter().filter(|p| !names.contains(p)).collect();
        if params.is_empty() {
            continue;
        }
        let Some(body) = decl.child_by_field_name("body") else { continue };
        let in_interface = decl.kind() == "interface_declaration";
        let owner = decl.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()).unwrap_or("?");
        let mut c = body.walk();
        for member in body.named_children(&mut c) {
            if is_static_context(member, in_interface, bytes) {
                visit(member, &params, owner, bytes, &mut out);
            }
        }
    }
    out
}

/// Whether a member of a type body has no instance of that type: a `static` field, method or
/// initializer, an interface field, or a nested type that is static — written so, or implicitly as
/// every nested interface, enum and record is, and every type nested in an interface.
fn is_static_context(member: Node, in_interface: bool, bytes: &[u8]) -> bool {
    match member.kind() {
        "field_declaration" => in_interface || has_keyword(member, bytes, "static"),
        "method_declaration" => has_keyword(member, bytes, "static"),
        "constant_declaration" | "static_initializer" => true,
        "class_declaration" => in_interface || has_keyword(member, bytes, "static"),
        "interface_declaration" | "enum_declaration" | "record_declaration" | "annotation_type_declaration" => true,
        _ => false,
    }
}

/// Report every `type_identifier` naming one of `params` under `node`, dropping the names a nearer
/// declaration re-declares as its own type parameters.
fn visit(node: Node, params: &[String], owner: &str, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let shadowing = type_parameter_names(node, bytes);
    let live: Vec<String> = params.iter().filter(|p| !shadowing.contains(p)).cloned().collect();
    if live.is_empty() {
        return;
    }
    if node.kind() == "type_identifier"
        && !node.parent().is_some_and(|p| matches!(p.kind(), "scoped_type_identifier" | "type_parameter"))
    {
        if let Ok(name) = node.utf8_text(bytes) {
            if live.iter().any(|p| p == name) {
                out.push(CheckId::StaticContextAccess.at(
                    node,
                    format!("Type parameter `{name}` of `{owner}` cannot be used in a static context"),
                ));
            }
        }
        return;
    }
    let mut c = node.walk();
    for child in node.named_children(&mut c) {
        visit(child, &live, owner, bytes, out);
    }
}

/// The names a declaration's own `<…>` introduces — empty for anything that has none.
fn type_parameter_names(decl: Node, bytes: &[u8]) -> Vec<String> {
    let Some(list) = decl.child_by_field_name("type_parameters") else { return Vec::new() };
    let mut c = list.walk();
    let names = list
        .named_children(&mut c)
        .filter(|p| p.kind() == "type_parameter")
        .filter_map(|p| crate::support::nodes::child_of_kind(p, "type_identifier"))
        .filter_map(|id| id.utf8_text(bytes).ok().map(str::to_string))
        .collect();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errs(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        static_type_var_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn static_members_cannot_use_the_class_parameter() {
        let src = "class G<T> { static T f; static T m(T v) { T local = null; return null; } static class N { T v; } }";
        assert_eq!(errs(src).len(), 5, "{:?}", errs(src));
    }

    #[test]
    fn instance_members_and_shadowing_parameters_are_fine() {
        let src = "class G<T> { T f; T m(T v) { return v; } static <T> T id(T t) { return t; } \
                   static <U> U other(U u) { return u; } class Inner { T v; } static class Own<T> { T v; } \
                   static G<String> make() { return new G<String>(); } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn a_type_named_like_the_parameter_is_left_alone() {
        assert!(errs("class G<T> { static T f; } class T {}").is_empty());
    }
}
