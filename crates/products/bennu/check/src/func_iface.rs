//! `@FunctionalInterface` SAM-count check — an interface annotated `@FunctionalInterface` that does
//! NOT declare exactly one abstract method. A functional interface is a Single-Abstract-Method (SAM)
//! type; zero abstract methods (`interface E {}`) or two-or-more non-overriding abstract methods
//! (`{ void a(); void b(); }`) is a compile error.
//!
//! This is the SAM-*count* half of the `@FunctionalInterface` story; `annotations.rs` already flags
//! the annotation sitting on a non-interface (target legality). The two never overlap: this pass only
//! looks INSIDE a real `interface_declaration`, `annotations.rs` only looks at WHERE the annotation
//! sits — so an `@FunctionalInterface class C {}` is reported once (by `annotations.rs`) and never here.
//!
//! Pure-AST and conservative (docs: NEVER a false positive):
//!   * we count only methods we can classify with certainty from the syntax tree;
//!   * a method's abstractness is read structurally: an interface method is abstract iff it has NO
//!     body (no `block`) and is NOT `default` / `static` / `private` (those all carry a body and don't
//!     contribute to the SAM);
//!   * `equals(Object)` / `hashCode()` / `toString()` may be redeclared by a functional interface and
//!     don't count toward the SAM — excluded by simple name + arity;
//!   * if the interface `extends` anything, it inherits abstract methods we can't see, so the full SAM
//!     set is uncountable → we SKIP entirely (never flag).
//!
//! Only a `@FunctionalInterface` interface with NO `extends` clause and a clearly-countable
//! abstract-method count `!= 1` is flagged.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag `@FunctionalInterface` interfaces whose abstract-method (SAM) count is not exactly one.
/// Pure-AST: iterates the shared `nodes` slice, no resolver.
pub fn func_iface_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "interface_declaration" {
            continue;
        }
        if !has_functional_interface_annotation(n, bytes) {
            continue;
        }
        // Inherited abstract methods are invisible from the syntax tree — can't count the full SAM
        // set soundly, so SKIP an interface that extends anything.
        if has_extends_clause(n) {
            continue;
        }
        let Some(body) = n.child_by_field_name("body") else { continue };
        let count = abstract_method_count(body, bytes);
        if count == 1 {
            continue;
        }
        let name = n
            .child_by_field_name("name")
            .and_then(|nm| nm.utf8_text(bytes).ok())
            .unwrap_or("this interface");
        let detail = if count == 0 {
            "it has no abstract method"
        } else {
            "it has multiple non-overriding abstract methods"
        };
        let anchor = n.child_by_field_name("name").unwrap_or(n);
        out.push(Diagnostic {
            message: format!(
                "`{name}` is not a functional interface: it must declare exactly one abstract method ({detail})"
            ),
            severity: "error".to_string(),
            code: String::new(),
            start: anchor.start_byte(),
            end: anchor.end_byte(),
        });
    }
    out
}

/// Whether a `marker_annotation` / `annotation` named `FunctionalInterface` (bare or
/// `java.lang.FunctionalInterface`) appears in the interface's `modifiers`. Mirrors `annotations.rs`.
fn has_functional_interface_annotation(node: Node, bytes: &[u8]) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() != "modifiers" {
            continue;
        }
        let mut mc = ch.walk();
        for m in ch.children(&mut mc) {
            if !matches!(m.kind(), "marker_annotation" | "annotation") {
                continue;
            }
            if let Some(name) = m.child_by_field_name("name") {
                if let Ok(t) = name.utf8_text(bytes) {
                    let simple = t.rsplit('.').next().unwrap_or(t);
                    if simple == "FunctionalInterface" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Whether the interface has an `extends A, B` clause (an `extends_interfaces` child).
fn has_extends_clause(node: Node) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "extends_interfaces" {
            return true;
        }
    }
    false
}

/// The count of ABSTRACT interface methods declared directly in `body`: a `method_declaration` with
/// NO body (no `block`) that is not `default` / `static` / `private`, and is not a redeclaration of a
/// `java.lang.Object` public method (`equals(Object)` / `hashCode()` / `toString()`, matched by simple
/// name + arity, which don't count toward the SAM).
fn abstract_method_count(body: Node, bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        if member.kind() != "method_declaration" {
            continue;
        }
        if is_abstract_sam_method(member, bytes) {
            count += 1;
        }
    }
    count
}

/// Whether one `method_declaration` in an interface body contributes to the SAM count.
fn is_abstract_sam_method(member: Node, bytes: &[u8]) -> bool {
    // A `block` body means a concrete (default/static/private) method — not abstract, doesn't count.
    let has_body = member.child_by_field_name("body").map(|b| b.kind() == "block").unwrap_or(false);
    if has_body {
        return false;
    }
    // Belt-and-suspenders: a `default` / `static` / `private` interface method carries a body, but if
    // one somehow appears bodiless (broken source) exclude it explicitly rather than miscount.
    for kw in modifier_keywords(member, bytes) {
        if matches!(kw, "default" | "static" | "private") {
            return false;
        }
    }
    // Redeclarations of `java.lang.Object` public methods don't count toward the SAM.
    !is_object_method(member, bytes)
}

/// Whether `member` redeclares a `java.lang.Object` public method that a functional interface may
/// restate without it counting: `equals(1 param)`, `hashCode(0)`, `toString(0)` — matched by simple
/// name + parameter arity only (conservative; never resolves the parameter type).
fn is_object_method(member: Node, bytes: &[u8]) -> bool {
    let Some(name) = member.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) else {
        return false;
    };
    let arity = param_arity(member);
    matches!((name, arity), ("equals", 1) | ("hashCode", 0) | ("toString", 0))
}

/// The number of formal parameters of a method (counts `formal_parameter` + `spread_parameter`).
fn param_arity(member: Node) -> usize {
    let Some(params) = member.child_by_field_name("parameters") else { return 0 };
    let mut n = 0;
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        if matches!(p.kind(), "formal_parameter" | "spread_parameter") {
            n += 1;
        }
    }
    n
}

/// The keyword modifiers (anonymous tokens) on a declaration — annotations (named nodes) excluded.
/// Same shape as `declarations.rs::modifier_keywords`.
fn modifier_keywords<'a>(node: Node, bytes: &'a [u8]) -> Vec<&'a str> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut out = Vec::new();
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() {
                    if let Ok(t) = m.utf8_text(bytes) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
    }
    Vec::new()
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

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        func_iface_errors_nodes(&crate::check::collect_nodes(tree.root_node()), src)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    // ── positives ───────────────────────────────────────────────────────────

    #[test]
    fn two_abstract_methods_is_flagged() {
        let e = errs(
            "@FunctionalInterface interface BrokenFunctionalInterface { void first_method(); void second_method(); }",
        );
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("not a functional interface"), "{e:?}");
        assert!(e[0].contains("multiple non-overriding abstract methods"), "{e:?}");
        assert!(e[0].contains("BrokenFunctionalInterface"), "{e:?}");
    }

    #[test]
    fn zero_abstract_methods_is_flagged() {
        let e = errs("@FunctionalInterface interface E {}");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("no abstract method"), "{e:?}");
    }

    #[test]
    fn only_default_and_static_methods_is_flagged() {
        // No abstract method at all — every method has a body.
        let e = errs("@FunctionalInterface interface E { default void x(){} static void y(){} }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("no abstract method"), "{e:?}");
    }

    #[test]
    fn qualified_annotation_name_is_recognised() {
        let e = errs("@java.lang.FunctionalInterface interface E {}");
        assert_eq!(e.len(), 1, "{e:?}");
    }

    // ── negatives (never a false positive) ──────────────────────────────────

    #[test]
    fn exactly_one_abstract_method_is_ok() {
        assert!(errs("@FunctionalInterface interface Ok { void run(); }").is_empty());
    }

    #[test]
    fn one_abstract_plus_defaults_and_object_method_is_ok() {
        let src = "@FunctionalInterface interface Ok2 { void run(); default void x(){} static void y(){} boolean equals(Object o); }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn all_three_object_methods_plus_one_sam_is_ok() {
        let src = "@FunctionalInterface interface Ok3 { void run(); boolean equals(Object o); int hashCode(); String toString(); }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn extends_clause_is_skipped() {
        // Inherited abstract methods are invisible → can't count the SAM set → never flag.
        assert!(errs("@FunctionalInterface interface I extends Other { void a(); void b(); }").is_empty());
        assert!(errs("@FunctionalInterface interface I extends Other {}").is_empty());
    }

    #[test]
    fn interface_without_annotation_is_never_flagged() {
        // Two abstract methods but NO `@FunctionalInterface` → a plain multi-method interface, legal.
        assert!(errs("interface I { void a(); void b(); }").is_empty());
        assert!(errs("interface E {}").is_empty());
    }

    #[test]
    fn functional_annotation_on_class_is_not_our_business() {
        // `annotations.rs` handles target legality; this pass only looks inside an interface, so a
        // class annotated `@FunctionalInterface` yields nothing here (no double report).
        assert!(errs("@FunctionalInterface class C {}").is_empty());
    }

    #[test]
    fn only_object_methods_no_sam_is_flagged() {
        // `equals`/`hashCode`/`toString` don't count → this interface has ZERO SAM methods.
        let e = errs(
            "@FunctionalInterface interface E { boolean equals(Object o); int hashCode(); String toString(); }",
        );
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("no abstract method"), "{e:?}");
    }
}
