//! Java **version-gated** feature checks — flag a language feature used below the version the
//! project targets (records need 16, sealed types 17, `var` 10, text blocks 15, switch arrows 14,
//! lambdas / method references 8, …). Legacy Struts/JSP projects are usually Java 8, so this catches
//! a paste from a newer codebase that won't compile.
//!
//! Pure AST + a target major version. `java_major` is the project's language level (e.g. `8` for
//! `1.8`); when it's unknown the caller skips this pass entirely.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Every feature-below-target-version error in `root`.
pub fn version_errors(root: Node, source: &str, java_major: u32) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Lombok's `var` (and `val`) back-port local type inference to pre-10 via an annotation
    // processor, so a `var` is legal on Java 8 when the file imports it. Detect the import once.
    let lombok_var = has_lombok_var_import(root, bytes);
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        if let Some((feature, min)) = feature_at(n, bytes) {
            // `var` provided by Lombok compiles below Java 10 — don't flag it.
            if lombok_var && feature.starts_with("`var`") {
                continue;
            }
            if java_major < min {
                let (start, end) = anchor(n, bytes);
                out.push(Diagnostic {
                    message: format!(
                        "{feature} require{} Java {min}, but the project targets Java {java_major}",
                        if feature.ends_with('s') { "" } else { "s" }
                    ),
                    severity: "error".to_string(),
                    start,
                    end,
                });
            }
        }
    }
    out
}

/// The `(feature label, min major version)` a node introduces, if any.
fn feature_at(n: Node, bytes: &[u8]) -> Option<(&'static str, u32)> {
    match n.kind() {
        "record_declaration" => Some(("Records", 16)),
        // A text block is its own node in some grammar builds, a triple-quoted `string_literal` in
        // others — match both.
        "text_block" => Some(("Text blocks", 15)),
        "string_literal" if n.utf8_text(bytes).map(|t| t.starts_with("\"\"\"")).unwrap_or(false) => {
            Some(("Text blocks", 15))
        }
        "lambda_expression" => Some(("Lambda expressions", 8)),
        "method_reference" => Some(("Method references", 8)),
        // Arrow-labelled / expression `switch` (`case X ->`).
        "switch_rule" => Some(("Switch expressions", 14)),
        "try_with_resources_statement" => Some(("Try-with-resources", 7)),
        // `var x = …` local type inference.
        "local_variable_declaration" => is_var(n, bytes).then_some(("`var` local variables", 10)),
        // A sealed / non-sealed type.
        "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
            if has_sealed_modifier(n, bytes) || has_permits(n) =>
        {
            Some(("Sealed types", 17))
        }
        // Multi-catch `catch (A | B e)`.
        "catch_type" if type_alternatives(n) > 1 => Some(("Multi-catch", 7)),
        // Interface methods: a body needs 8 (`default`/`static`), a `private` one needs 9.
        "method_declaration" => interface_method_feature(n, bytes),
        _ => None,
    }
}

/// A tight span for the diagnostic — the node clamped to its first line (a whole record/lambda body
/// squiggled red is noise).
fn anchor(n: Node, bytes: &[u8]) -> (usize, usize) {
    let start = n.start_byte();
    let mut end = n.end_byte().min(bytes.len());
    if start < end {
        if let Some(nl) = bytes[start..end].iter().position(|&b| b == b'\n') {
            end = start + nl;
        }
    }
    (start, end.max(start))
}

fn is_var(n: Node, bytes: &[u8]) -> bool {
    n.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) == Some("var")
}

/// Whether the file imports Lombok's `var`/`val` (`import lombok.var;`, `lombok.val`, or `lombok.*`),
/// which makes a `var` legal below Java 10.
fn has_lombok_var_import(root: Node, bytes: &[u8]) -> bool {
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() == "import_declaration" {
            if let Ok(t) = child.utf8_text(bytes) {
                let t = t.replace(char::is_whitespace, "");
                if t.contains("lombok.var") || t.contains("lombok.val") || t.contains("lombok.*") {
                    return true;
                }
            }
        }
    }
    false
}

fn has_sealed_modifier(n: Node, bytes: &[u8]) -> bool {
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() && matches!(m.utf8_text(bytes), Ok("sealed") | Ok("non-sealed")) {
                    return true;
                }
            }
        }
    }
    false
}

fn has_permits(n: Node) -> bool {
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        if ch.kind() == "permits" {
            return true;
        }
    }
    false
}

fn type_alternatives(catch_type: Node) -> usize {
    let mut c = catch_type.walk();
    let mut count = 0;
    for ch in catch_type.named_children(&mut c) {
        let k = ch.kind();
        if k.ends_with("type") || k == "type_identifier" || k == "scoped_type_identifier" {
            count += 1;
        }
    }
    count
}

/// Interface-method version features: a body → 8, `private` → 9. `None` outside an interface.
fn interface_method_feature(n: Node, bytes: &[u8]) -> Option<(&'static str, u32)> {
    // Only interface methods.
    let mut cur = n.parent();
    let in_interface = loop {
        match cur {
            Some(p) if p.kind() == "interface_declaration" => break true,
            Some(p)
                if matches!(
                    p.kind(),
                    "class_declaration" | "enum_declaration" | "record_declaration"
                ) =>
            {
                break false
            }
            Some(p) => cur = p.parent(),
            None => break false,
        }
    };
    if !in_interface {
        return None;
    }
    let mut c = n.walk();
    let mut is_private = false;
    for ch in n.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() && m.utf8_text(bytes) == Ok("private") {
                    is_private = true;
                }
            }
        }
    }
    if is_private {
        return Some(("Private interface methods", 9));
    }
    let has_body = n.child_by_field_name("body").map(|b| b.kind() == "block").unwrap_or(false);
    if has_body {
        Some(("Default/static interface methods", 8))
    } else {
        None
    }
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

    fn at(src: &str, major: u32) -> Vec<String> {
        let tree = parse(src);
        version_errors(tree.root_node(), src, major).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn record_needs_16() {
        assert_eq!(at("record R(int x) {}", 8).len(), 1);
        assert!(at("record R(int x) {}", 17).is_empty());
    }

    #[test]
    fn var_needs_10() {
        let src = "class C { void m() { var x = 1; } }";
        assert_eq!(at(src, 8).len(), 1);
        assert!(at(src, 11).is_empty());
        // An explicit type is never flagged.
        assert!(at("class C { void m() { int x = 1; } }", 8).is_empty());
    }

    #[test]
    fn var_across_jdk_levels() {
        // The same `var` local: flagged on 8 and 9, fine from 10 up.
        let src = "class C { void m() { var x = 1; } }";
        assert_eq!(at(src, 8).len(), 1, "Java 8");
        assert_eq!(at(src, 9).len(), 1, "Java 9");
        assert!(at(src, 10).is_empty(), "Java 10");
        assert!(at(src, 11).is_empty(), "Java 11");
        assert!(at(src, 17).is_empty(), "Java 17");
    }

    #[test]
    fn lombok_var_is_allowed_below_10() {
        // Lombok back-ports `var`, so an import of it clears the version error even on Java 8.
        let with_import = "import lombok.var;\nclass C { void m() { var x = 1; } }";
        assert!(at(with_import, 8).is_empty(), "lombok.var import → allowed on 8");
        let wildcard = "import lombok.*;\nclass C { void m() { var x = 1; } }";
        assert!(at(wildcard, 8).is_empty(), "lombok.* import → allowed on 8");
        // Without the import it's still flagged.
        assert_eq!(at("class C { void m() { var x = 1; } }", 8).len(), 1);
    }

    #[test]
    fn lambda_and_method_ref_need_8() {
        let src = "class C { java.util.function.Supplier<String> s = () -> \"x\"; }";
        assert_eq!(at(src, 7).len(), 1);
        assert!(at(src, 8).is_empty());
        let mr = "class C { Runnable r = this::m; void m() {} }";
        assert!(at(mr, 7).iter().any(|m| m.contains("Method references")));
    }

    #[test]
    fn text_block_needs_15() {
        let src = "class C { String s = \"\"\"\nhi\"\"\"; }";
        assert_eq!(at(src, 8).len(), 1);
        assert!(at(src, 17).is_empty());
    }

    #[test]
    fn switch_arrow_needs_14() {
        let src = "class C { void m(int x) { switch (x) { case 1 -> {} default -> {} } } }";
        assert!(at(src, 8).iter().any(|m| m.contains("Switch")));
        assert!(at(src, 17).is_empty());
        // Classic colon switch is fine on 8.
        assert!(at("class C { void m(int x) { switch (x) { case 1: break; } } }", 8).is_empty());
    }

    #[test]
    fn sealed_needs_17() {
        assert!(at("sealed class C permits D {} final class D extends C {}", 8).iter().any(|m| m.contains("Sealed")));
        assert!(at("sealed interface I {}", 11).iter().any(|m| m.contains("Sealed")));
    }

    #[test]
    fn try_with_resources_needs_7() {
        let src = "class C { void m() throws Exception { try (AutoCloseable a = null) {} } }";
        assert!(at(src, 6).iter().any(|m| m.contains("Try-with-resources")));
        assert!(at(src, 8).is_empty());
    }

    #[test]
    fn private_interface_method_needs_9_default_needs_8() {
        assert!(at("interface I { private void h() {} }", 8).iter().any(|m| m.contains("Private interface")));
        assert!(at("interface I { default void h() {} }", 7).iter().any(|m| m.contains("Default/static")));
        assert!(at("interface I { default void h() {} }", 8).is_empty());
        // A plain abstract interface method has no body → no version feature.
        assert!(at("interface I { void h(); }", 6).is_empty());
    }

    #[test]
    fn plain_java_8_code_is_clean() {
        let src = "class C { int add(int a, int b) { return a + b; } }";
        assert!(at(src, 8).is_empty());
    }
}
