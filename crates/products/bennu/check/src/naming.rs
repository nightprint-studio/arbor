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

/// A `public` top-level type that disagrees with the name of the file holding it.
///
/// The shape both consumers of this rule need: the **diagnostic** says the file is wrong, and the
/// **intention** offers the two ways out — rename the type, or rename the file. Neither can invent
/// the other's half, so the finding is a value rather than a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFileMismatch {
    /// The type's declared name.
    pub name: String,
    /// The keyword it was declared with (`class`, `interface`, `enum`, `record`, `@interface`) —
    /// what a label calls it, so an offer does not say "class" about an enum.
    pub keyword: &'static str,
    /// Byte span of the NAME token, which is what a rename replaces.
    pub start: usize,
    pub end: usize,
}

/// Every `public` top-level type in `root` whose name differs from `file_stem` (the file name
/// without its `.java` extension). Empty when the caller has no file name to compare against.
pub fn type_file_mismatches(root: Node, source: &str, file_stem: &str) -> Vec<TypeFileMismatch> {
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
            out.push(TypeFileMismatch {
                name: name.to_string(),
                keyword: keyword_of(child.kind()),
                start: name_node.start_byte(),
                end: name_node.end_byte(),
            });
        }
    }
    out
}

/// What the declaration is called in prose — the word an offer's label uses.
fn keyword_of(kind: &str) -> &'static str {
    match kind {
        "interface_declaration" => "interface",
        "enum_declaration" => "enum",
        "record_declaration" => "record",
        "annotation_type_declaration" => "@interface",
        _ => "class",
    }
}

/// Flag each `public` top-level type whose name differs from `file_stem` (the file name without its
/// `.java` extension).
pub fn class_name_matches_file(root: Node, source: &str, file_stem: &str) -> Vec<Diagnostic> {
    type_file_mismatches(root, source, file_stem)
        .into_iter()
        .map(|m| Diagnostic {
            message: format!(
                "Public type `{}` must be declared in a file named `{}.java`",
                m.name, m.name
            ),
            severity: crate::check_id::CheckId::TypeNameMismatchFile.severity().to_string(),
            code: crate::check_id::CheckId::TypeNameMismatchFile.code().to_string(),
            start: m.start,
            end: m.end,
        })
        .collect()
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
    fn the_mismatch_carries_the_name_span_a_rename_would_replace() {
        let src = "public class Foo {}";
        let tree = parse(src);
        let found = type_file_mismatches(tree.root_node(), src, "Bar");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Foo");
        assert_eq!(&src[found[0].start..found[0].end], "Foo");
    }

    #[test]
    fn the_keyword_is_what_the_declaration_actually_said() {
        // An offer that calls an enum a "class" is an offer that reads as a bug in the tool.
        for (src, want) in [
            ("public class Foo {}", "class"),
            ("public interface Foo {}", "interface"),
            ("public enum Foo { A }", "enum"),
            ("public record Foo(int x) {}", "record"),
            ("public @interface Foo {}", "@interface"),
        ] {
            let tree = parse(src);
            let found = type_file_mismatches(tree.root_node(), src, "Bar");
            assert_eq!(found.first().map(|m| m.keyword), Some(want), "{src}");
        }
    }

    #[test]
    fn nested_public_class_is_not_a_top_level_mismatch() {
        // A public NESTED type doesn't have to match the file name — only top-level ones.
        assert!(check("public class Foo { public class Bar {} }", "Foo").is_empty());
    }
}
