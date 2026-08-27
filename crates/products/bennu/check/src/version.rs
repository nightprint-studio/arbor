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
    version_errors_nodes(root, &crate::check::collect_nodes(root), source, java_major)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
/// `root` is still taken for the one-shot Lombok-`var`-import pre-scan.
pub fn version_errors_nodes(root: Node, nodes: &[Node], source: &str, java_major: u32) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Lombok's `var` (and `val`) back-port local type inference to pre-10 via an annotation
    // processor, so a `var` is legal on Java 8 when the file imports it. Detect the import once.
    let lombok_var = has_lombok_var_import(root, bytes);
    let mut out = Vec::new();
    for &n in nodes {
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
                    severity: crate::check_id::CheckId::FeatureRequiresNewerJava.severity().to_string(),
                    code: crate::check_id::CheckId::FeatureRequiresNewerJava.code().to_string(),
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
        // A colon-form `switch` *expression* has no `switch_rule` — what dates it is the `yield`.
        // Only one *inside a switch* counts: `yield` is a contextual keyword, so a pre-14 project
        // calling its own `yield(x)` method parses as a `yield_statement` too, and flagging that
        // would be a false positive on code that compiles.
        "yield_statement" => inside_switch(n).then_some(("`yield`", 14)),
        // What a `switch` accepts widened across releases; a case label is where each widening shows.
        "switch_label" => switch_label_feature(n),
        "try_with_resources_statement" => Some(("Try-with-resources", 7)),
        // `var x = …` local type inference.
        "local_variable_declaration" => is_var(n, bytes).then_some(("`var` local variables", 10)),
        // A sealed / non-sealed type. (`record_declaration` is handled above — records are final,
        // never sealed — so it's intentionally not repeated here.)
        "class_declaration" | "interface_declaration" | "enum_declaration"
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

/// The dated feature a `case` label uses, if any.
///
/// `switch` started out accepting only the int family; each release since widened either the
/// **selector** or the **label**, and both widenings are visible in the label itself — which is what
/// makes this decidable without a resolver:
///
///   * a **string literal** label can only sit on a `String` selector → Java 7;
///   * a **type or record pattern** label → Java 21 (preview from 17, final in 21);
///   * a **`when` guard** → Java 21, and it is a *sibling* of its pattern in the grammar, so it is
///     matched in its own right rather than assumed to travel with one;
///   * **`case null`** → Java 21. Before it, `null` was the one selector value a `switch` could not
///     be given a label for, and reaching one threw instead.
///
/// The *selector* widenings that leave no mark on any label (an `enum` or a boxed `Integer`, both
/// Java 5) are not gated here: no project Bennu opens targets Java 1.4.
fn switch_label_feature(label: Node) -> Option<(&'static str, u32)> {
    let mut c = label.walk();
    for ch in label.named_children(&mut c) {
        match ch.kind() {
            "pattern" | "type_pattern" | "record_pattern" => {
                return Some(("`switch` type patterns", 21))
            }
            "guard" => return Some(("Pattern guards", 21)),
            "null_literal" => return Some(("`case null`", 21)),
            // A text block is its own node in some grammar builds and a triple-quoted
            // `string_literal` in others; either spelling means a `String` selector.
            "string_literal" | "text_block" => return Some(("`switch` on `String`", 7)),
            _ => {}
        }
    }
    None
}

/// Whether `n` sits anywhere inside a `switch` — the guard that tells a `switch` `yield` from a call
/// to a method a pre-14 project happened to name `yield`.
fn inside_switch(n: Node) -> bool {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if p.kind() == "switch_expression" {
            return true;
        }
        cur = p.parent();
    }
    false
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

    // ── what `switch` accepts, by release ────────────────────────────────────

    #[test]
    fn a_string_switch_needs_7() {
        let src = "class C { void m(String s) { switch (s) { case \"a\": break; } } }";
        let d = at(src, 6);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`switch` on `String` requires Java 7"), "{d:?}");
        assert!(at(src, 8).is_empty());
    }

    #[test]
    fn an_int_switch_is_clean_on_every_release() {
        let src = "class C { void m(int i) { switch (i) { case 1: break; default: break; } } }";
        assert!(at(src, 5).is_empty());
    }

    #[test]
    fn case_null_needs_21() {
        let src = "class C { void m(String s) { switch (s) { case null: break; default: break; } } }";
        let d = at(src, 17);
        assert!(d.iter().any(|m| m.contains("`case null` requires Java 21")), "{d:?}");
        assert!(at(src, 21).is_empty(), "{:?}", at(src, 21));
    }

    #[test]
    fn a_type_pattern_label_needs_21() {
        let src = "class C { void m(Object o) { switch (o) { case String s -> {} default -> {} } } }";
        let d = at(src, 17);
        assert!(d.iter().any(|m| m.contains("type patterns require Java 21")), "{d:?}");
        assert!(at(src, 21).is_empty(), "{:?}", at(src, 21));
    }

    #[test]
    fn a_when_guard_needs_21() {
        let src = "class C { void m(Object o, boolean f) { switch (o) { case String s when f -> {} default -> {} } } }";
        let d = at(src, 17);
        assert!(d.iter().any(|m| m.contains("Java 21")), "{d:?}");
        assert!(at(src, 21).is_empty(), "{:?}", at(src, 21));
    }

    #[test]
    fn a_switch_yield_needs_14() {
        let src = "class C { int m(int i) { return switch (i) { default: yield 1; }; } }";
        let d = at(src, 11);
        assert!(d.iter().any(|m| m.contains("`yield` requires Java 14")), "{d:?}");
        assert!(at(src, 17).is_empty(), "{:?}", at(src, 17));
    }

    /// `yield` is a contextual keyword: a pre-14 project may have its own `yield(…)` method, which
    /// parses as a `yield_statement` too. Only one inside a `switch` dates the file.
    #[test]
    fn a_call_to_a_method_named_yield_is_not_a_switch_yield() {
        let src = "class C { void yield(int x) {} void m() { yield(1); } }";
        assert!(at(src, 8).is_empty(), "{:?}", at(src, 8));
    }
}
