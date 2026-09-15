//! Locating a type's declaration in one source file by its **binary name**, nesting included.
//!
//! A binary name carries the whole path to a type — `org/acme/RestClient$UriSpec` from bytecode,
//! `org/acme/RestClient/UriSpec` from source — and every search that reduced it to its last segment
//! answered a different question: *some* type called `UriSpec` in this file. One file can declare two
//! of those (`Outer.Inner` and `Other.Inner`), and a `$` left in the "simple" name matched nothing at
//! all. Here the path is matched as a path: the outer type, then the nested one inside it.

use tree_sitter::Node;

use crate::symbols::{anonymous_type_name, is_anonymous_body};

/// Every node kind that declares a named type.
pub const TYPE_DECLARATION_KINDS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// Whether `node` declares a named type (class, interface, enum, record, annotation).
pub fn is_type_declaration(node: &Node) -> bool {
    TYPE_DECLARATION_KINDS.contains(&node.kind())
}

/// The innermost simple name of a binary name, whichever spelling it uses:
/// `com/acme/Outer$Inner` and `com/acme/Outer/Inner` are both `Inner`.
pub fn binary_simple_name(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

/// The names from the file's outermost type down to `decl` itself — `[Outer, Inner]`.
///
/// An anonymous class on the way contributes the ordinal javac (and the extractor) gives it, so a
/// type declared inside one reads `[Outer, 1, Local]`, the path its binary name spells. `None` when a
/// name on the path cannot be read.
pub fn type_nesting(decl: &Node, bytes: &[u8]) -> Option<Vec<String>> {
    let mut names = vec![decl.child_by_field_name("name")?.utf8_text(bytes).ok()?.to_string()];
    let mut cur = decl.parent();
    while let Some(n) = cur {
        if is_type_declaration(&n) {
            names.push(n.child_by_field_name("name")?.utf8_text(bytes).ok()?.to_string());
        } else if is_anonymous_body(&n) {
            names.push(anonymous_type_name(&n, bytes)?);
        }
        cur = n.parent();
    }
    names.reverse();
    Some(names)
}

/// The package a compilation unit declares, dotted (`com.acme`), if it declares one.
pub fn declared_package(root: &Node, bytes: &[u8]) -> Option<String> {
    crate::spans::package_name(root, bytes)
}

/// The binary name this file gives the type `decl` declares: its package, then its nesting,
/// `/`-separated — the spelling the source extractor files it under.
pub fn declared_type_binary(decl: &Node, bytes: &[u8], package: Option<&str>) -> Option<String> {
    let nesting = type_nesting(decl, bytes)?.join("/");
    Some(match package {
        Some(p) => format!("{}/{nesting}", p.replace('.', "/")),
        None => nesting,
    })
}

/// The declaration of the type `binary` names, in the tree under `root`.
///
/// A declaration matches when its nesting path is the tail of the binary name. When several do —
/// a top-level `Inner` and `Outer.Inner` both end `…/Inner` — the one the file's package makes exact
/// wins, then the longer path: `p/Outer/Inner` is `Outer.Inner`, never a top-level namesake. `None`
/// when no declaration's path fits, which the caller reads as "not declared here".
pub fn find_type_declaration<'t>(root: &Node<'t>, bytes: &[u8], binary: &str) -> Option<Node<'t>> {
    let segments: Vec<&str> = binary.split(['/', '$']).filter(|s| !s.is_empty()).collect();
    let package = crate::spans::package_name(root, bytes);
    let package: Vec<&str> = package.as_deref().map(|p| p.split('.').collect()).unwrap_or_default();

    // (exact, path length, start byte) — higher exact/length wins, then the earlier declaration.
    let mut best: Option<((bool, usize, std::cmp::Reverse<usize>), Node<'t>)> = None;
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if !is_type_declaration(&n) {
            continue;
        }
        // Cheap reject before the parent walk: the declaration's own name must be the last segment.
        let own = n.child_by_field_name("name").and_then(|nm| nm.utf8_text(bytes).ok());
        if own != segments.last().copied() {
            continue;
        }
        let Some(nesting) = type_nesting(&n, bytes) else { continue };
        if nesting.len() > segments.len() {
            continue;
        }
        let tail = &segments[segments.len() - nesting.len()..];
        if !tail.iter().zip(&nesting).all(|(a, b)| *a == b) {
            continue;
        }
        let head = &segments[..segments.len() - nesting.len()];
        let rank = (head == package.as_slice(), nesting.len(), std::cmp::Reverse(n.start_byte()));
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, n));
        }
    }
    best.map(|(_, n)| n)
}

/// The NAME token of the declaration of the type `binary` names in `source` — nesting-aware
/// [`crate::spans::find_type_name_span`].
///
/// Falls back to the first type of the innermost simple name when no declaration's path fits (a
/// local type javac numbers, `Outer$1Helper`), which is exactly what the simple search answered.
pub fn find_binary_type_name_span(source: &str, binary: &str) -> Option<(usize, usize)> {
    let simple = binary_simple_name(binary);
    if !source.contains(simple) {
        return None;
    }
    let tree = crate::grammar::parse_java(source)?;
    let found = find_type_declaration(&tree.root_node(), source.as_bytes(), binary)
        .and_then(|decl| decl.child_by_field_name("name"))
        .map(|nm| (nm.start_byte(), nm.end_byte()));
    found.or_else(|| crate::spans::find_type_name_span(source, simple))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_INNERS: &str = "package p;\n\
        public class Outer {\n    public static class Inner { void run() {} }\n}\n\
        class Other {\n    static class Inner { void run() {} }\n}\n\
        class Inner {}\n";

    fn name_at(source: &str, binary: &str) -> Option<usize> {
        find_binary_type_name_span(source, binary).map(|(s, _)| s)
    }

    #[test]
    fn the_simple_name_of_either_spelling_is_the_innermost() {
        assert_eq!(binary_simple_name("org/acme/RestClient$UriSpec"), "UriSpec");
        assert_eq!(binary_simple_name("org/acme/RestClient/UriSpec"), "UriSpec");
        assert_eq!(binary_simple_name("Bare"), "Bare");
    }

    #[test]
    fn a_nested_type_is_found_inside_its_own_outer() {
        let outer_inner = TWO_INNERS.find("class Inner {").unwrap() + "class ".len();
        // The LAST `static class Inner` is `Other`'s.
        let other_inner = TWO_INNERS.rfind("static class Inner").unwrap() + "static class ".len();
        assert_eq!(name_at(TWO_INNERS, "p/Outer/Inner"), Some(outer_inner));
        assert_eq!(name_at(TWO_INNERS, "p/Outer$Inner"), Some(outer_inner));
        assert_eq!(name_at(TWO_INNERS, "p/Other$Inner"), Some(other_inner));
    }

    #[test]
    fn a_top_level_type_is_not_taken_for_a_nested_namesake() {
        let top = TWO_INNERS.rfind("class Inner {}").unwrap() + "class ".len();
        assert_eq!(name_at(TWO_INNERS, "p/Inner"), Some(top));
    }

    #[test]
    fn a_library_nested_interface_is_found_from_its_bytecode_name() {
        let src = "package org.springframework.web.client;\n\
                   public interface RestClient {\n    interface UriSpec<S> { S uri(); }\n}\n";
        let at = name_at(src, "org/springframework/web/client/RestClient$UriSpec").unwrap();
        assert!(src[at..].starts_with("UriSpec<S>"));
    }

    #[test]
    fn declared_binary_reads_package_and_nesting() {
        let tree = crate::grammar::parse_java(TWO_INNERS).unwrap();
        let root = tree.root_node();
        let decl = find_type_declaration(&root, TWO_INNERS.as_bytes(), "p/Other$Inner").unwrap();
        assert_eq!(
            declared_type_binary(&decl, TWO_INNERS.as_bytes(), Some("p")).as_deref(),
            Some("p/Other/Inner")
        );
    }

    #[test]
    fn an_unknown_path_falls_back_to_the_simple_name() {
        let src = "package p;\nclass Helper {}\n";
        assert_eq!(name_at(src, "p/Outer$1Helper"), None, "no `1Helper` declared");
        assert_eq!(name_at(src, "q/Helper"), Some(src.find("Helper").unwrap()));
    }
}
