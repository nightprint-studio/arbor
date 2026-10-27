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
    version_errors_nodes(root, &crate::engine::check::collect_nodes(root), source, java_major)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
/// `root` is still taken for the one-shot Lombok-`var`-import pre-scan.
pub fn version_errors_nodes(root: Node, nodes: &[Node], source: &str, java_major: u32) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Lombok's `var` back-ports local type inference to pre-10 via an annotation processor, so a
    // `var` is legal on Java 8 when the file imports it (`import lombok.var;`, or a `lombok.*`
    // wildcard). Detected once, from the shared gate — `import lombok.val;` alone is a different
    // keyword and does not make `var` legal.
    let lombok_var =
        crate::support::lombok::imports_keyword("var", &crate::support::lombok::imports_from_root(root, bytes));
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
                    severity: crate::engine::check_id::CheckId::FeatureRequiresNewerJava.severity().to_string(),
                    code: crate::engine::check_id::CheckId::FeatureRequiresNewerJava.code().to_string(),
                    start,
                    end,
                });
                if n.kind() == "record_declaration" {
                    out.extend(record_read_as_a_method(n));
                }
            }
        }
    }
    out
}

/// What javac says next about an empty `record` below Java 16: it does not know the word, so it reads
/// `record Point(int x) { }` as a METHOD called `Point` returning a type called `record`, whose empty
/// body falls off the end without a `return`. The feature error above is the cause; this is javac's
/// own follow-up, worded so it explains itself — on the closing brace, where javac puts it.
///
/// Only for a body with nothing in it. With members the method reading gives javac a pile of unrelated
/// syntax errors, and repeating those would be noise on top of the one error that matters.
fn record_read_as_a_method(record: Node) -> Option<Diagnostic> {
    let body = record.child_by_field_name("body")?;
    let mut c = body.walk();
    let has_members = body.named_children(&mut c).any(|ch| !matches!(ch.kind(), "line_comment" | "block_comment"));
    let close = body.child(body.child_count().checked_sub(1)?).filter(|last| last.kind() == "}");
    let close = close.filter(|_| !has_members)?;
    Some(crate::engine::check_id::CheckId::MissingReturn.at(
        close,
        "Missing return statement: below Java 16 `record` is not a keyword, so this declaration is read as a method returning a type named `record`",
    ))
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
        // Java 22 (JEP 456): `_` as a pattern, a local, a catch or a lambda parameter. Below it `_`
        // is a reserved word (9+) and the file does not compile.
        "underscore_pattern" => Some(("Unnamed variables and patterns", 22)),
        // `x instanceof String s` (JEP 394): the binding name is what makes it a pattern, and what
        // dates it. A record pattern is its own node, below.
        "instanceof_expression" if n.child_by_field_name("name").is_some() => {
            Some(("`instanceof` patterns", 16))
        }
        // `x instanceof Point(int a, int b)` (JEP 440). One inside a `case` label is dated by the label
        // already; a nested one by the outermost.
        "record_pattern" if n.parent().is_some_and(|p| p.kind() == "instanceof_expression") => {
            Some(("Record patterns", 21))
        }
        // Java 25 (JEP 511).
        "module_import_declaration" => Some(("`import module`", 25)),
        // Java 25 (JEP 513): a statement ahead of `this(…)` / `super(…)`. javac's own wording below
        // it is "call to super must be first statement in constructor".
        "explicit_constructor_invocation" if follows_a_statement(n) => {
            Some(("Statements before `this(…)` / `super(…)`", 25))
        }
        _ => None,
    }
}

/// Whether a **statement** precedes the explicit constructor invocation `n` in its constructor body.
///
/// A comment is a named node in this grammar, and one sitting between the opening brace and `this(…)`
/// is the ordinary way to explain which constructor is being delegated to — so it is not a statement
/// and must not date the file to Java 25.
fn follows_a_statement(n: Node) -> bool {
    let mut prev = n.prev_named_sibling();
    while let Some(p) = prev {
        if !matches!(p.kind(), "line_comment" | "block_comment") {
            return true;
        }
        prev = p.prev_named_sibling();
    }
    false
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
///   * **several labels in one `case`** (`case 6, 7:`) → Java 14. When the label also carries a newer
///     feature (`case null, default`), the newer one is the one reported.
///
/// The *selector* widenings that leave no mark on any label (an `enum` or a boxed `Integer`, both
/// Java 5) are not gated here: no project Bennu opens targets Java 1.4.
fn switch_label_feature(label: Node) -> Option<(&'static str, u32)> {
    let widest = widest_label_feature(label);
    if !has_several_labels(label) {
        return widest;
    }
    match widest {
        Some((_, min)) if min >= 14 => widest,
        _ => Some(("Multiple `case` labels", 14)),
    }
}

/// Whether one `case` lists more than one constant (`case 1, 2`, `case null, default`).
fn has_several_labels(label: Node) -> bool {
    let mut c = label.walk();
    let listed = label
        .named_children(&mut c)
        .filter(|ch| !matches!(ch.kind(), "guard" | "annotation" | "marker_annotation"))
        .count();
    listed > 1
}

/// The newest feature a single label's own operand uses (pattern, guard, `null`, string).
fn widest_label_feature(label: Node) -> Option<(&'static str, u32)> {
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
    fn the_java_22_to_25_syntax_is_gated() {
        let flexible = "class C extends B { C(int x) { int y = x; super(y); } }";
        assert_eq!(at(flexible, 21).len(), 1);
        assert!(at(flexible, 25).is_empty());
        // The first statement is the ordinary form.
        assert!(at("class C extends B { C(int x) { super(x); int y = x; } }", 8).is_empty());

        assert_eq!(at("import module java.base;\nclass C {}", 21).len(), 1);
        assert!(at("import module java.base;\nclass C {}", 25).is_empty());

        let unnamed = "class C { void m(Object o) { if (o instanceof P(int x, _)) {} } }";
        assert_eq!(at(unnamed, 21).len(), 1);
        assert!(at(unnamed, 22).is_empty());
    }

    #[test]
    fn a_comment_before_this_or_super_is_not_a_statement() {
        let line_comments = "class C { C() {\n  // NB: uses the general constructor\n  //     see C(String)\n  this(\"x\");\n } C(String s) {} }";
        assert!(at(line_comments, 8).is_empty());
        let block_comment = "class C extends B { C() { /* delegate */ super(1); } }";
        assert!(at(block_comment, 8).is_empty());
        // A real statement still dates it, comments or not between it and the call.
        let statement = "class C extends B { C(int x) { int y = x; // then\n super(y); } }";
        assert_eq!(at(statement, 8).len(), 1);
        assert!(at(statement, 25).is_empty());
    }

    #[test]
    fn record_needs_16() {
        // The gate itself, plus javac's follow-up for an empty record (see
        // `an_empty_record_below_16_also_gets_javacs_missing_return`).
        let below = at("record R(int x) {}", 8);
        assert!(below.iter().any(|m| m.contains("Records require Java 16")), "{below:?}");
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
    fn several_labels_in_one_case_need_14() {
        let src = "class C { void m(int d) { switch (d) { case 6, 7: break; default: break; } } }";
        let d = at(src, 11);
        assert!(d.iter().any(|m| m.contains("Multiple `case` labels require Java 14")), "{d:?}");
        assert!(at(src, 14).is_empty(), "{:?}", at(src, 14));
        // A single label is clean on every release, and `case null, default` is dated by the `null`.
        assert!(at("class C { void m(int d) { switch (d) { case 6: break; default: break; } } }", 8).is_empty());
        let with_null = "class C { void m(String s) { switch (s) { case null, default -> {} } } }";
        let d = at(with_null, 17);
        assert!(d.iter().any(|m| m.contains("`case null` requires Java 21")), "{d:?}");
        // Two strings are Java 7's feature AND the multi-label one: the later release is reported.
        let strings = "class C { void m(String s) { switch (s) { case \"a\", \"b\": break; default: break; } } }";
        assert!(at(strings, 11).iter().any(|m| m.contains("Java 14")), "{:?}", at(strings, 11));
        assert!(at(strings, 14).is_empty());
    }

    #[test]
    fn an_empty_record_below_16_also_gets_javacs_missing_return() {
        let src = "class C {\n  record P(int x) {\n  }\n}";
        let d = at(src, 11);
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(d[0].contains("Records require Java 16"), "{d:?}");
        assert!(d[1].contains("Missing return statement"), "{d:?}");
        // At 16 the record is a record and nothing is added; a record with members only gets the gate.
        assert!(at(src, 16).is_empty(), "{:?}", at(src, 16));
        let with_members = "class C { record P(int x) { int twice() { return x * 2; } } }";
        assert_eq!(at(with_members, 11).len(), 1, "{:?}", at(with_members, 11));
    }

    #[test]
    fn an_instanceof_pattern_needs_16() {
        let src = "class C { boolean m(Object o) { return o instanceof String s && !s.isEmpty(); } }";
        let d = at(src, 15);
        assert!(d.iter().any(|m| m.contains("`instanceof` patterns require Java 16")), "{d:?}");
        assert!(at(src, 16).is_empty(), "{:?}", at(src, 16));
        // The plain type test has no binding and is clean everywhere.
        assert!(at("class C { boolean m(Object o) { return o instanceof String; } }", 8).is_empty());
    }

    #[test]
    fn a_record_pattern_in_instanceof_needs_21() {
        let src = "class C { boolean m(Object o) { return o instanceof Point(int x, int y); } }";
        let d = at(src, 17);
        assert!(d.iter().any(|m| m.contains("Record patterns require Java 21")), "{d:?}");
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
