//! Validation — a call on a **type**: `Files.copy(…)`, `Order.of(…)`, `MyEnum.values()`.
//!
//! The unknown-member check only ever looked at a receiver it could infer as a VALUE, so a static
//! call was checked by nothing at all. That is worse than it sounds: a `var` bound to a call that
//! does not exist has no type, so every call made on the result went unchecked too — one silent
//! miss took a whole statement chain with it.
//!
//! Widening a check is where false positives come from, so most of this file is the other
//! direction: the static calls that are perfectly ordinary and must stay silent. A synthesized
//! member, an inherited static, a nested qualifier, a fully-qualified name — each is a shape that
//! resolves through a different path, and each would report a "method that does not exist" on
//! code that compiles.

mod common;
use common::*;

fn errors_in(body: &str) -> Vec<String> {
    let p = Project::new(&[
        (
            "Util.java",
            "package shop;\n\
             public class Util {\n\
             \x20   public static String tag() { return \"\"; }\n\
             \x20   public static class Nested {\n\
             \x20       public static int size() { return 0; }\n\
             \x20   }\n\
             }\n",
        ),
        (
            "Base.java",
            "package shop;\n\
             public class Base {\n\
             \x20   public static String shared() { return \"\"; }\n\
             }\n",
        ),
        (
            "Sub.java",
            "package shop;\npublic class Sub extends Base {\n}\n",
        ),
        (
            "Colour.java",
            "package shop;\npublic enum Colour {\n    RED, BLUE;\n}\n",
        ),
        (
            "Use.java",
            &format!("package shop;\npublic class Use {{\n    void go() {{\n{body}\n    }}\n}}\n"),
        ),
    ]);
    p.validate_errors("Use.java")
}

/// `validate_errors` answers `"code: message"`, so a test asks about the CODE by prefix.
fn flags_unknown_member(body: &str) -> bool {
    errors_in(body).iter().any(|e| e.starts_with("unknown-member"))
}

// ── The miss ─────────────────────────────────────────────────────────────────────────────────

/// The headline: a static method that does not exist is now said so.
#[test]
fn a_static_method_that_does_not_exist_is_flagged() {
    assert!(flags_unknown_member("        Util.nope();"), "{:?}", errors_in("        Util.nope();"));
}

/// The reported case. `MyProva` is a nested CLASS, and calling it without `new` is a call to a
/// method nobody wrote — which is exactly what it looks like to the compiler too.
#[test]
fn a_nested_type_called_as_a_method_is_flagged() {
    assert!(flags_unknown_member("        Util.Nested();"));
}

// ── The other direction, which is most of the work ───────────────────────────────────────────

#[test]
fn a_static_method_that_exists_is_silent() {
    assert!(errors_in("        Util.tag();").is_empty());
}

/// Through the nested type's own name — the qualifier is `Util.Nested`, a dotted name that has to
/// resolve as one thing.
#[test]
fn a_static_on_a_nested_type_is_silent() {
    assert!(errors_in("        Util.Nested.size();").is_empty());
}

/// A static is inherited. The walk goes up, as it does for everything else.
#[test]
fn a_static_inherited_from_a_superclass_is_silent() {
    assert!(errors_in("        Sub.shared();").is_empty());
}

/// `values()` and `valueOf(…)` are written by the language, not by anybody's source — a check that
/// only reads declarations reports both as missing on every enum in the project.
#[test]
fn an_enums_implicit_statics_are_silent() {
    assert!(errors_in("        Colour[] all = Colour.values();").is_empty());
    assert!(errors_in("        Colour c = Colour.valueOf(\"RED\");").is_empty());
}

/// `System.out` is a field, and `println` is on what it holds. If the receiver cannot be typed the
/// check must stay silent rather than read `System.out` as a type name.
#[test]
fn a_call_through_a_static_field_is_silent() {
    assert!(errors_in("        System.out.println(\"x\");").is_empty());
}

/// The receiver is a value, and reading it as a type must not happen at all. This is the path that
/// was already there, and it has to keep working in both directions.
///
/// A PROJECT type, deliberately: this fixture's JDK is a stand-in with a handful of methods on it,
/// so a `String` receiver would answer about the fixture rather than about the check. The real JDK
/// is exercised in `validation_static_calls_real_jdk`.
#[test]
fn an_ordinary_instance_call_is_unaffected() {
    assert!(errors_in("        Sub s = new Sub();\n        s.shared();").is_empty());
    assert!(flags_unknown_member("        Sub s = new Sub();\n        s.nope();"));
}

/// A type the index has never seen has no members to judge against, and a check that assumed
/// otherwise would report every call into an un-indexed dependency.
#[test]
fn an_unresolvable_receiver_is_silent() {
    assert!(errors_in("        Unknown.whatever();").is_empty());
}
