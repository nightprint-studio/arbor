//! Declaration-site **name spans** and **binary-name** lookups over a single `.java` source.
//!
//! These are pure tree-sitter-java CST scans — no resolver, no project, no filesystem — so they
//! belong to the source model (this crate), not to the query engine. Two consumers reach here:
//!   * go-to-declaration / rename (`bennu-intel`) locate a type declaration's NAME token via
//!     [`find_type_name_span`];
//!   * inherited-members (`bennu-query`) resolves a type's JVM binary name by `(simple, line)` via
//!     [`binary_of_type_at`], and locates a supertype's project source via [`find_type_name_span`].

use tree_sitter::Node;

/// Find the byte span of a type declaration's NAME token in `source` (class / interface / enum /
/// record / annotation matching `simple`). `None` when `source` declares no type with that simple
/// name. (A same-named type in another package could match; the caller scans the project's sources
/// and the first hit wins — good enough for navigation, and classification already narrowed the
/// caret to this binary name.)
pub fn find_type_name_span(source: &str, simple: &str) -> Option<(usize, usize)> {
    // Skip the parse when the type name isn't even present in the file (a cheap early-out for
    // callers that probe more than one file). A substring false-positive just parses one file that
    // then yields no match — correct, only slightly slower.
    if !source.contains(simple) {
        return None;
    }
    let tree = crate::grammar::parse_java(source)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            if let Some(nm) = n.child_by_field_name("name") {
                if nm.utf8_text(bytes).ok() == Some(simple) {
                    return Some((nm.start_byte(), nm.end_byte()));
                }
            }
        }
    }
    None
}

/// Resolve the JVM **binary name** (package + nesting, slash-separated) of the class / interface /
/// enum named `simple` whose declaration name token sits on 1-based `line`. The line disambiguates
/// a nested / same-simple-named type; when `line <= 0` (the caller couldn't pin a line), the first
/// same-named declaration wins. `None` when `source` declares no matching type.
///
/// Nested types are keyed with a `/` separator (`Outer/Inner`), matching the source extractor's
/// FQN persisted in the index — NOT the JVM `Outer$Inner` form — so a project record lookup hits.
pub fn binary_of_type_at(source: &str, simple: &str, line: i64) -> Option<String> {
    let tree = crate::grammar::parse_java(source)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    // Package + nested-type context tracked as we descend, so `Outer.Inner` binds correctly.
    let package = package_name(&root, bytes);
    let mut found: Option<String> = None;
    walk_types(&root, bytes, package.as_deref(), None, simple, line, &mut found);
    found
}

/// Recursive type walk building each declaration's binary name; on a name+line match, set `found`
/// (first match wins — the caller's line already disambiguated).
#[allow(clippy::too_many_arguments)]
fn walk_types(
    node: &Node,
    bytes: &[u8],
    package: Option<&str>,
    outer_binary: Option<&str>,
    simple: &str,
    line: i64,
    found: &mut Option<String>,
) {
    let mut cur = node.walk();
    for child in node.named_children(&mut cur) {
        // The same five kinds the other two walks in this file already list. Missing `record` and
        // `@interface` here meant their binary names were never built, so go-to could not locate
        // the declaration and fell back to opening it as an external library view — a nested
        // record in the file you are already looking at, reported as "not in this project".
        if matches!(
            child.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            let Some(name_node) = child.child_by_field_name("name") else { continue };
            let Ok(name) = name_node.utf8_text(bytes) else { continue };
            let binary = match outer_binary {
                Some(o) => format!("{o}/{name}"),
                None => match package {
                    Some(p) => format!("{}/{name}", p.replace('.', "/")),
                    None => name.to_string(),
                },
            };
            if found.is_none() && name == simple {
                // 1-based line of the name token.
                let name_line = name_node.start_position().row as i64 + 1;
                if line <= 0 || name_line == line {
                    *found = Some(binary.clone());
                }
            }
            // Descend into the body for nested types.
            if let Some(body) = child.child_by_field_name("body") {
                walk_types(&body, bytes, package, Some(&binary), simple, line, found);
            }
        } else {
            // A non-type container (e.g. the compilation unit) — descend for top-level types.
            walk_types(&child, bytes, package, outer_binary, simple, line, found);
        }
        if found.is_some() {
            return;
        }
    }
}

/// The JVM **binary name** (package + nesting, `/`-separated) of the **innermost type declaration**
/// whose byte range contains `byte_offset` — i.e. the type the caret is writing inside. `None` when
/// the offset sits in no type body (file header, imports, between top-level types).
///
/// Used by member-access completion to decide **private** member visibility: a `private` member is
/// accessible only from within its own top-level class (JLS §6.6.1), so completion compares this
/// against each candidate member's declaring type. Same `/`-nesting convention as
/// [`binary_of_type_at`] (`Outer/Inner`), so it lines up with indexed project binaries.
pub fn enclosing_type_binary(source: &str, byte_offset: usize) -> Option<String> {
    let tree = crate::grammar::parse_java(source)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let package = package_name(&root, bytes);
    let mut found: Option<String> = None;
    walk_enclosing(&root, bytes, package.as_deref(), None, byte_offset, &mut found);
    found
}

/// Recursive walk tracking the innermost type declaration containing `offset` — descending into a
/// containing type's body overwrites `found` with the nested binary, so the deepest wins.
fn walk_enclosing(
    node: &Node,
    bytes: &[u8],
    package: Option<&str>,
    outer_binary: Option<&str>,
    offset: usize,
    found: &mut Option<String>,
) {
    let mut cur = node.walk();
    for child in node.named_children(&mut cur) {
        if matches!(
            child.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            let Some(name_node) = child.child_by_field_name("name") else { continue };
            let Ok(name) = name_node.utf8_text(bytes) else { continue };
            let binary = match outer_binary {
                Some(o) => format!("{o}/{name}"),
                None => match package {
                    Some(p) => format!("{}/{name}", p.replace('.', "/")),
                    None => name.to_string(),
                },
            };
            if offset >= child.start_byte() && offset < child.end_byte() {
                *found = Some(binary.clone());
                if let Some(body) = child.child_by_field_name("body") {
                    walk_enclosing(&body, bytes, package, Some(&binary), offset, found);
                }
            }
        } else {
            walk_enclosing(&child, bytes, package, outer_binary, offset, found);
        }
    }
}

/// The package name of a compilation unit, if declared.
fn package_name(root: &Node, bytes: &[u8]) -> Option<String> {
    let mut cur = root.walk();
    for child in root.children(&mut cur) {
        if child.kind() == "package_declaration" {
            let mut pc = child.walk();
            for n in child.named_children(&mut pc) {
                if matches!(n.kind(), "scoped_identifier" | "identifier") {
                    return n.utf8_text(bytes).ok().map(|s| s.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_span_points_at_the_name_token() {
        let src = "package com.acme;\npublic class Order {\n}\n";
        let (s, e) = find_type_name_span(src, "Order").expect("span");
        assert_eq!(&src[s..e], "Order");
        // A type not declared here → None.
        assert!(find_type_name_span(src, "Customer").is_none());
    }

    #[test]
    fn type_name_span_matches_records_and_interfaces() {
        assert!(find_type_name_span("interface Foo {}", "Foo").is_some());
        assert!(find_type_name_span("record Point(int x, int y) {}", "Point").is_some());
        assert!(find_type_name_span("@interface Ann {}", "Ann").is_some());
        assert!(find_type_name_span("enum E { A, B }", "E").is_some());
    }

    #[test]
    fn binary_of_type_at_finds_a_nested_record() {
        // `find_type_name_span` above already knew records; THIS walk did not, so a nested record
        // had no binary name — and go-to, which resolves through it, reported a type declared in
        // the open file as belonging to an external library.
        let src = "package com.acme;\npublic class Compiler {\n    private record failure(String why) {}\n}\n";
        let line = src[..src.find("record failure").unwrap()].lines().count() as i64;
        assert_eq!(
            binary_of_type_at(src, "failure", line),
            Some("com/acme/Compiler/failure".to_string())
        );
    }

    #[test]
    fn binary_of_type_at_finds_a_nested_annotation_type() {
        let src = "package com.acme;\npublic class Holder {\n    public @interface Marker {}\n}\n";
        let line = src[..src.find("@interface Marker").unwrap()].lines().count() as i64;
        assert_eq!(
            binary_of_type_at(src, "Marker", line),
            Some("com/acme/Holder/Marker".to_string())
        );
    }

    #[test]
    fn binary_of_type_at_matches_by_line() {
        let src = "package com.acme;\npublic class Order {\n}\n";
        // `Order` name token is on line 2.
        assert_eq!(binary_of_type_at(src, "Order", 2).as_deref(), Some("com/acme/Order"));
        // A wrong line yields no match.
        assert!(binary_of_type_at(src, "Order", 99).is_none());
        // line <= 0 → first same-named decl wins.
        assert_eq!(binary_of_type_at(src, "Order", 0).as_deref(), Some("com/acme/Order"));
    }

    #[test]
    fn binary_of_nested_type_uses_slash_separator() {
        let src = "package com.acme;\nclass Outer {\n  class Inner {\n  }\n}\n";
        // Inner's name token is on line 3.
        assert_eq!(binary_of_type_at(src, "Inner", 3).as_deref(), Some("com/acme/Outer/Inner"));
    }

    #[test]
    fn binary_of_type_without_package() {
        let src = "class Bare {\n}\n";
        assert_eq!(binary_of_type_at(src, "Bare", 1).as_deref(), Some("Bare"));
    }

    #[test]
    fn enclosing_type_binary_finds_innermost() {
        let src = "package com.acme;\nclass Outer {\n  void m() {\n    int x = 0;\n  }\n  class Inner {\n    int y = 1;\n  }\n}\n";
        // Offset inside `m()`'s body (the `int x` line) → Outer.
        let in_m = src.find("int x").unwrap();
        assert_eq!(enclosing_type_binary(src, in_m).as_deref(), Some("com/acme/Outer"));
        // Offset inside Inner's body (`int y`) → the nested binary.
        let in_inner = src.find("int y").unwrap();
        assert_eq!(enclosing_type_binary(src, in_inner).as_deref(), Some("com/acme/Outer/Inner"));
        // Offset in the file header (the package line) → not inside any type.
        assert!(enclosing_type_binary(src, 3).is_none());
    }
}
