//! Special compilation-unit files — `package-info.java` and `module-info.java`. Java gives these two
//! names a *restricted* grammar the normal `.java` rules don't cover, so they get their own check:
//!
//!   * **`package-info.java`** may hold only a package declaration (with its annotations) and the
//!     imports those annotations need — no type declarations. It also *must* declare a package
//!     (that's its entire reason to exist: a home for package-level annotations / Javadoc).
//!   * **`module-info.java`** holds a single `module { … }` declaration — no package, no types.
//!
//! Driven by `file_stem` (the file's base name), so it only runs on the two magic names and never
//! touches an ordinary file.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// Validate a `package-info.java` / `module-info.java` file. A no-op (`[]`) for every other
/// `file_stem`, so the caller can always call it.
pub fn special_file_errors(root: Node, source: &str, file_stem: &str) -> Vec<Diagnostic> {
    match file_stem {
        "package-info" => package_info_errors(root, source),
        "module-info" => module_info_errors(root, source),
        _ => Vec::new(),
    }
}

/// `package-info.java`: exactly one package declaration, no type declarations.
fn package_info_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut has_package = false;
    let mut c = root.walk();
    for child in root.children(&mut c) {
        match child.kind() {
            "package_declaration" => has_package = true,
            k if TYPE_DECLS.contains(&k) => {
                let anchor = child.child_by_field_name("name").unwrap_or(child);
                let what = type_label(&child, bytes);
                out.push(err(
                    format!(
                        "`package-info.java` may only contain the package declaration and its \
                         annotations — move {what} to its own file"
                    ),
                    anchor,
                    bytes,
                ));
            }
            _ => {}
        }
    }
    if !has_package && !has_error(root) {
        // No package at all — the file has no purpose. Anchor at the first thing in the file (or the
        // whole empty root). Suppressed while the buffer is mid-edit (a parse error anywhere), so a
        // half-typed file doesn't flash this.
        let anchor = root.named_child(0).unwrap_or(root);
        out.push(err(
            "`package-info.java` must contain a package declaration".to_string(),
            anchor,
            bytes,
        ));
    }
    out
}

/// `module-info.java`: a single module declaration, nothing else.
fn module_info_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut has_module = false;
    let mut c = root.walk();
    for child in root.children(&mut c) {
        match child.kind() {
            "module_declaration" => has_module = true,
            "package_declaration" => out.push(err(
                "`module-info.java` cannot declare a package".to_string(),
                child,
                bytes,
            )),
            k if TYPE_DECLS.contains(&k) => {
                let anchor = child.child_by_field_name("name").unwrap_or(child);
                out.push(err(
                    format!(
                        "`module-info.java` may only contain the module declaration — move {} to \
                         its own file",
                        type_label(&child, bytes)
                    ),
                    anchor,
                    bytes,
                ));
            }
            _ => {}
        }
    }
    if !has_module && !has_error(root) {
        let anchor = root.named_child(0).unwrap_or(root);
        out.push(err(
            "`module-info.java` must contain a module declaration".to_string(),
            anchor,
            bytes,
        ));
    }
    out
}

/// A readable label for a type declaration node: `class `Foo`` / `interface `Bar``.
fn type_label(node: &Node, bytes: &[u8]) -> String {
    let kw = match node.kind() {
        "class_declaration" => "class",
        "interface_declaration" => "interface",
        "enum_declaration" => "enum",
        "record_declaration" => "record",
        "annotation_type_declaration" => "annotation",
        _ => "type",
    };
    match node.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) {
        Some(name) => format!("{kw} `{name}`"),
        None => format!("the {kw}"),
    }
}

/// Whether any node in the tree is an `ERROR`/`MISSING` — used to stay silent while the file is
/// mid-edit (the syntax check already reports the parse error; we don't pile a spurious
/// "must contain …" on top).
fn has_error(root: Node) -> bool {
    root.has_error()
}

fn err(message: String, node: Node, bytes: &[u8]) -> Diagnostic {
    let start = node.start_byte();
    // Clamp a multi-line node (a whole type decl) to its first line so we don't paint the gutter.
    let end = bytes[start..node.end_byte()]
        .iter()
        .position(|&b| b == b'\n')
        .map(|nl| start + nl)
        .unwrap_or(node.end_byte());
    Diagnostic { message, severity: crate::check_id::CheckId::SpecialFileContent.severity().to_string(), code: crate::check_id::CheckId::SpecialFileContent.code().to_string(), start, end }
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
        special_file_errors(tree.root_node(), src, stem).into_iter().map(|d| d.message).collect()
    }

    // ── package-info.java ──────────────────────────────────────────────────────

    #[test]
    fn clean_package_info_is_ok() {
        assert!(check("@Deprecated\npackage com.acme;\n", "package-info").is_empty());
    }

    #[test]
    fn package_info_with_imports_and_annotation_is_ok() {
        let src = "import javax.annotation.ParametersAreNonnullByDefault;\n\
                   @ParametersAreNonnullByDefault\npackage com.acme.web;\n";
        assert!(check(src, "package-info").is_empty());
    }

    #[test]
    fn package_info_with_a_class_is_flagged() {
        let src = "package com.acme;\nclass Helper {}\n";
        let e = check(src, "package-info");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("Helper") && e[0].contains("own file"), "{e:?}");
    }

    #[test]
    fn package_info_without_package_is_flagged() {
        // A lone annotation import but no `package …;` — the file is pointless.
        let e = check("import java.util.List;\n", "package-info");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("must contain a package"), "{e:?}");
    }

    #[test]
    fn package_info_interface_is_flagged() {
        let src = "package com.acme;\npublic interface Marker {}\n";
        let e = check(src, "package-info");
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("interface `Marker`"), "{e:?}");
    }

    // ── module-info.java ───────────────────────────────────────────────────────

    #[test]
    fn clean_module_info_is_ok() {
        assert!(check("module com.acme {\n  requires java.base;\n}\n", "module-info").is_empty());
    }

    #[test]
    fn module_info_with_a_type_is_flagged() {
        let src = "module com.acme {}\nclass Sneaky {}\n";
        let e = check(src, "module-info");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("Sneaky"), "{e:?}");
    }

    #[test]
    fn module_info_without_module_is_flagged() {
        let e = check("class Foo {}\n", "module-info");
        // one for the type, one for the missing module decl
        assert!(e.iter().any(|m| m.contains("must contain a module")), "{e:?}");
    }

    // ── ordinary files untouched ───────────────────────────────────────────────

    #[test]
    fn ordinary_file_is_never_touched() {
        assert!(check("package com.acme;\nclass Foo {}\n", "Foo").is_empty());
    }
}
