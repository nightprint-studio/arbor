//! File-name / public-type agreement — a `public` top-level type must be declared in a file whose
//! base name matches it (JLS §7.6). `Foo.java` may only hold a `public class Foo` (or interface /
//! enum / record / annotation `Foo`).
//!
//! Needs the file's base name (without `.java`), so it's the one check that takes context beyond the
//! source. When the caller has no file name (a scratch buffer), it's skipped.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// Flag each `public` top-level type whose name differs from `file_stem` (the file name without its
/// `.java` extension).
pub fn class_name_matches_file(root: Node, source: &str, file_stem: &str) -> Vec<Diagnostic> {
    if file_stem.is_empty() {
        return Vec::new();
    }
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if !TYPE_DECLS.contains(&child.kind()) || !is_public(child, bytes) {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else { continue };
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        if name != file_stem {
            out.push(Diagnostic {
                message: format!("Public type `{name}` must be declared in a file named `{name}.java`"),
                severity: crate::check_id::CheckId::TypeNameMismatchFile.severity().to_string(),
                code: crate::check_id::CheckId::TypeNameMismatchFile.code().to_string(),
                start: name_node.start_byte(),
                end: name_node.end_byte(),
            });
        }
    }
    out
}

/// Whether a top-level declaration carries the `public` modifier.
fn is_public(node: Node, bytes: &[u8]) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() && m.utf8_text(bytes) == Ok("public") {
                    return true;
                }
            }
        }
    }
    false
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

    fn check(src: &str, stem: &str) -> Vec<String> {
        let tree = parse(src);
        class_name_matches_file(tree.root_node(), src, stem)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn matching_public_class_is_ok() {
        assert!(check("public class Foo {}", "Foo").is_empty());
    }

    #[test]
    fn mismatched_public_class_is_flagged() {
        let e = check("public class Foo {}", "Bar");
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("Foo.java"), "{e:?}");
    }

    #[test]
    fn non_public_class_is_not_required_to_match() {
        assert!(check("class Foo {}", "Bar").is_empty());
    }

    #[test]
    fn public_interface_enum_record_are_checked() {
        assert_eq!(check("public interface Foo {}", "Bar").len(), 1);
        assert_eq!(check("public enum Foo { A }", "Bar").len(), 1);
        assert_eq!(check("public record Foo(int x) {}", "Bar").len(), 1);
    }

    #[test]
    fn empty_stem_skips_the_check() {
        assert!(check("public class Foo {}", "").is_empty());
    }

    #[test]
    fn nested_public_class_is_not_a_top_level_mismatch() {
        // A public NESTED type doesn't have to match the file name — only top-level ones.
        assert!(check("public class Foo { public class Bar {} }", "Foo").is_empty());
    }
}
