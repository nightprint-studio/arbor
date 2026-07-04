//! Package / file-location agreement — the declared `package …;` must match the directory the file
//! lives in under its source root (IntelliJ's "package does not correspond to file path").
//!
//! `expected_package` is computed by the caller from the file path (via
//! [`bennu_java::prelude::infer_package`]); this module compares it to the declared package and, for
//! the quick-fix, produces the edit that rewrites the declaration. The **move-file** alternative is
//! a filesystem action handled in the be layer.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// Flag a declared package that doesn't match `expected` (the package inferred from the file's
/// location). `expected` is assumed non-empty (the caller skips the check when the location yields no
/// package — a default-package or non-source-root file).
pub fn package_mismatch(root: Node, source: &str, expected: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    match declared_package(root, bytes) {
        Some((declared, name_node)) if declared != expected => vec![Diagnostic {
            message: format!(
                "Package `{declared}` does not match the file's location (expected `{expected}`)"
            ),
            severity: "error".to_string(),
            start: name_node.start_byte(),
            end: name_node.end_byte(),
        }],
        None => {
            // No package declared, but the location expects one → anchor at the first type name.
            let anchor = first_type_name(root).unwrap_or(root);
            vec![Diagnostic {
                message: format!("Missing package declaration (the file's location expects `{expected}`)"),
                severity: "error".to_string(),
                start: anchor.start_byte(),
                end: anchor.end_byte(),
            }]
        }
        _ => Vec::new(), // matches
    }
}

/// The edit that makes the declared package `expected`: rewrite an existing `package …;`, or insert
/// one at the top when there is none. `None` when the package already matches (nothing to do).
/// Returns `(start_byte, end_byte, replacement)` — a byte-range splice the editor applies.
pub fn change_package_edit(root: Node, source: &str, expected: &str) -> Option<(usize, usize, String)> {
    let bytes = source.as_bytes();
    match declared_package(root, bytes) {
        Some((declared, _)) if declared == expected => None,
        Some((_, name_node)) => {
            // Replace just the name in `package <name>;` — keeps any leading annotations/spacing.
            Some((name_node.start_byte(), name_node.end_byte(), expected.to_string()))
        }
        None => {
            // Insert before the first import or type declaration (after a license header comment).
            let pos = insertion_point(root);
            Some((pos, pos, format!("package {expected};\n\n")))
        }
    }
}

/// Source-based wrapper over [`change_package_edit`] — parses `source` itself, for a caller (the
/// intentions handler) that only has the text. `None` when the package already matches or the parse
/// fails.
pub fn change_package(source: &str, expected: &str) -> Option<(usize, usize, String)> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source, None)?;
    change_package_edit(tree.root_node(), source, expected)
}

/// The declared package name (dotted) + its name node, or `None` for the default package.
fn declared_package<'a>(root: Node<'a>, bytes: &[u8]) -> Option<(String, Node<'a>)> {
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() == "package_declaration" {
            let mut pc = child.walk();
            for n in child.named_children(&mut pc) {
                if matches!(n.kind(), "scoped_identifier" | "identifier") {
                    if let Ok(t) = n.utf8_text(bytes) {
                        return Some((t.to_string(), n));
                    }
                }
            }
        }
    }
    None
}

/// The name node of the first top-level type declaration (for anchoring a missing-package error).
fn first_type_name(root: Node) -> Option<Node> {
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if TYPE_DECLS.contains(&child.kind()) {
            return child.child_by_field_name("name");
        }
    }
    None
}

/// Where to insert a package declaration: at the start of the first import or type declaration
/// (leaving any leading comment header above it). Byte 0 when the file has neither.
fn insertion_point(root: Node) -> usize {
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() == "import_declaration" || TYPE_DECLS.contains(&child.kind()) {
            return child.start_byte();
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn mismatches(src: &str, expected: &str) -> Vec<String> {
        let tree = parse(src);
        package_mismatch(tree.root_node(), src, expected).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn matching_package_is_ok() {
        assert!(mismatches("package com.acme.web;\nclass Foo {}", "com.acme.web").is_empty());
    }

    #[test]
    fn wrong_package_is_flagged() {
        let m = mismatches("package com.acme.web;\nclass Foo {}", "com.acme.model");
        assert_eq!(m.len(), 1);
        assert!(m[0].contains("com.acme.web") && m[0].contains("com.acme.model"), "{m:?}");
    }

    #[test]
    fn missing_package_is_flagged() {
        let m = mismatches("class Foo {}", "com.acme");
        assert_eq!(m.len(), 1);
        assert!(m[0].contains("Missing package"), "{m:?}");
    }

    #[test]
    fn change_edit_rewrites_existing_package() {
        let src = "package com.acme.web;\nclass Foo {}";
        let tree = parse(src);
        let (start, end, repl) = change_package_edit(tree.root_node(), src, "com.acme.model").unwrap();
        assert_eq!(&src[start..end], "com.acme.web");
        assert_eq!(repl, "com.acme.model");
    }

    #[test]
    fn change_edit_inserts_when_missing() {
        let src = "import java.util.List;\nclass Foo {}";
        let tree = parse(src);
        let (start, end, repl) = change_package_edit(tree.root_node(), src, "com.acme").unwrap();
        assert_eq!(start, end, "an insertion is zero-width");
        assert_eq!(&src[..start], ""); // before the import
        assert!(repl.starts_with("package com.acme;"), "{repl:?}");
    }

    #[test]
    fn change_edit_none_when_matching() {
        let src = "package com.acme;\nclass Foo {}";
        let tree = parse(src);
        assert!(change_package_edit(tree.root_node(), src, "com.acme").is_none());
    }
}
