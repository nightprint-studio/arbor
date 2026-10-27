//! Import-clash diagnostics — two SINGLE-TYPE imports that bind the SAME simple name to DIFFERENT
//! fully-qualified types (`import java.util.List;` + `import java.awt.List;`). Both would introduce
//! `List` into scope, so the second is a hard compile error ("collides with already imported").
//!
//! Purely syntactic — read off the written import list alone, no resolver:
//!   * only plain single-type imports are considered — `import static …` and wildcard `import a.b.*;`
//!     are skipped (a static member / an open set of types, not one bound simple name);
//!   * the simple name is the last `.`-segment of the imported FQN;
//!   * two single-type imports with the SAME simple name but DIFFERENT FQNs → the second is flagged;
//!   * two BYTE-IDENTICAL imports (same FQN twice) are a plain duplicate, NOT a clash — left to
//!     `imports.rs::duplicate_imports`, so an identical repeat is never flagged here.
//!
//! Never a false positive: a clash needs two DIFFERENT FQNs mapping to the same last segment, which is
//! an unambiguous compile error under any classpath.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag the second of two single-type imports that bind the same simple name to different FQNs.
/// Root-based, top-level children only (mirrors the other import checks in this crate).
pub fn import_clash_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // simple name → the FIRST FQN seen for it (only single-type, non-static, non-wildcard imports).
    let mut first_fqn: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    let (declared_here, package) = own_types_and_package(root, bytes);
    let mut ambiguous: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() != "import_declaration" {
            continue;
        }
        let Some((fqn, simple)) = single_type_import(child, bytes) else {
            continue;
        };
        // A top-level type of this very file that the import would also bind
        // (`already.defined.this.unit`) — unless the import names that same type.
        if declared_here.contains(&simple) && fqn != format!("{package}{simple}") {
            out.push(crate::engine::check_id::CheckId::DuplicateDeclaration.at(
                child,
                format!("Import `{fqn}` clashes with the type `{simple}` declared in this file"),
            ));
            continue;
        }
        match first_fqn.get(&simple) {
            None => {
                first_fqn.insert(simple, fqn);
            }
            Some(prior) if *prior == fqn => {
                // Byte-identical FQN — a duplicate, not a clash. Leave it to `duplicate_imports`.
            }
            Some(prior) => {
                ambiguous.insert(simple.clone());
                out.push(Diagnostic {
                    message: format!(
                        "Import `{fqn}` collides with `{prior}` (both bind `{simple}`)"
                    ),
                    severity: crate::engine::check_id::CheckId::ImportCollision.severity().to_string(),
                    code: crate::engine::check_id::CheckId::ImportCollision.code().to_string(),
                    start: child.start_byte(),
                    end: child.end_byte(),
                });
            }
        }
    }
    out.extend(ambiguous_uses(root, bytes, &ambiguous));
    out
}

/// Every use of a simple name that two single-type imports both bind: javac cannot choose and reports
/// `ref.ambiguous` at each one. Import lines themselves are not uses.
fn ambiguous_uses(root: Node, bytes: &[u8], names: &std::collections::HashSet<String>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if names.is_empty() {
        return out;
    }
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "import_declaration" {
            continue;
        }
        if n.kind() == "type_identifier" {
            if let Some(name) = n.utf8_text(bytes).ok().filter(|t| names.contains(*t)) {
                out.push(crate::engine::check_id::CheckId::AmbiguousReference.at(
                    n,
                    format!("Reference to `{name}` is ambiguous: two imports bind it"),
                ));
            }
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c));
    }
    out
}

/// The names of the top-level types this file declares, and its package as a `a.b.` prefix (empty
/// for the default package).
fn own_types_and_package(root: Node, bytes: &[u8]) -> (std::collections::HashSet<String>, String) {
    let mut types = std::collections::HashSet::new();
    let mut package = String::new();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        match child.kind() {
            "package_declaration" => {
                let mut pc = child.walk();
                let name = child
                    .named_children(&mut pc)
                    .find(|n| matches!(n.kind(), "scoped_identifier" | "identifier"))
                    .and_then(|n| n.utf8_text(bytes).ok());
                if let Some(name) = name {
                    package = format!("{name}.");
                }
            }
            "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
            | "annotation_type_declaration" => {
                if let Some(name) = child.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) {
                    types.insert(name.to_string());
                }
            }
            _ => {}
        }
    }
    (types, package)
}

/// For a single-type (non-static, non-wildcard) `import a.b.C;` return `(FQN, simple name)`.
/// `None` for static imports, wildcard imports, or any import we can't read a name off of.
/// Mirrors the node parsing in `imports.rs`: a `static` child ⇒ static, an `asterisk` child (or a
/// `.*` in the text, belt-and-suspenders) ⇒ wildcard, otherwise the `scoped_identifier`/`identifier`
/// is the dotted FQN and its `name` field the simple name.
fn single_type_import(import: Node, bytes: &[u8]) -> Option<(String, String)> {
    let is_wildcard_text = import.utf8_text(bytes).map(|t| t.contains(".*")).unwrap_or(false);

    let mut is_static = false;
    let mut is_wildcard = is_wildcard_text;
    let mut fqn: Option<String> = None;
    let mut simple: Option<String> = None;
    let mut c = import.walk();
    for part in import.children(&mut c) {
        match part.kind() {
            "static" => is_static = true,
            "asterisk" => is_wildcard = true,
            "scoped_identifier" => {
                fqn = part.utf8_text(bytes).ok().map(str::to_string);
                if let Some(name) = part.child_by_field_name("name") {
                    simple = name.utf8_text(bytes).ok().map(str::to_string);
                }
            }
            "identifier" => {
                // A bare single-segment import (`import Foo;`) — FQN and simple name coincide.
                let t = part.utf8_text(bytes).ok().map(str::to_string);
                fqn = t.clone();
                simple = t;
            }
            _ => {}
        }
    }
    if is_static || is_wildcard {
        return None;
    }
    Some((fqn?, simple?))
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

    fn clashes(src: &str) -> Vec<String> {
        let tree = parse(src);
        import_clash_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn an_import_of_a_type_this_file_declares_is_flagged_unless_it_is_that_type() {
        assert_eq!(clashes("import java.util.Deque;\nclass Deque { }").len(), 1);
        assert!(clashes("package a.b;\nimport a.b.Own;\nclass Own { }").is_empty());
        assert_eq!(clashes("package a.b;\nimport c.d.Own;\nclass Own { }").len(), 1);
    }

    #[test]
    fn same_simple_name_different_fqn_is_flagged() {
        let src = "import java.util.List;\nimport java.awt.List;\nclass C {}";
        let d = clashes(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("java.awt.List"), "flags the second import: {d:?}");
        assert!(d[0].contains("java.util.List"), "names the prior binding: {d:?}");
        assert!(d[0].contains("List"), "{d:?}");
    }

    #[test]
    fn different_simple_names_are_not_flagged() {
        let src = "import java.util.List;\nimport java.util.Map;\nclass C {}";
        assert!(clashes(src).is_empty());
    }

    #[test]
    fn identical_imports_are_not_flagged_here() {
        // Byte-identical FQN → a plain duplicate, `duplicate_imports`' job, not a clash.
        let src = "import java.util.List;\nimport java.util.List;\nclass C {}";
        assert!(clashes(src).is_empty(), "identical FQNs are a duplicate, not a clash");
    }

    #[test]
    fn static_imports_are_skipped() {
        let src = "import static java.util.Collections.sort;\nimport static java.util.Arrays.sort;\nclass C {}";
        assert!(clashes(src).is_empty(), "static imports bind members, not types");
    }

    #[test]
    fn wildcard_imports_are_skipped() {
        let src = "import java.util.*;\nimport java.awt.*;\nclass C {}";
        assert!(clashes(src).is_empty(), "wildcards are an open set, not one bound name");
    }
}
