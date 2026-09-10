//! Validation — a nested type written through its outer: `Outer.Inner`.
//!
//! `Cfg.MyProva` parses as one `scoped_type_identifier`, and the unresolved-type check skipped
//! every segment of one. That rule is right for `com.acme.Foo` — neither `com` nor `acme` is a
//! type, and flagging them would report a package — but it meant a nested type that does not exist
//! was silent wherever it was written qualified, which is the ordinary way to write one from
//! outside. `new Cfg.MyProva()` on a class with no `MyProva` reported nothing at all.
//!
//! Most of this file is the other direction, because that is where a check like this goes wrong.

mod common;
use common::*;

fn errors_in(body: &str) -> Vec<String> {
    let p = Project::new(&[
        (
            "Base.java",
            "package shop;\npublic class Base {\n    public static class Shared { }\n}\n",
        ),
        (
            "Cfg.java",
            "package shop;\n\
             public class Cfg extends Base {\n\
             \x20   public static class Inner { }\n\
             }\n",
        ),
        (
            "Use.java",
            &format!("package shop;\npublic class Use {{\n    void go() {{\n{body}\n    }}\n}}\n"),
        ),
    ]);
    p.validate_errors("Use.java")
}

fn flags_unresolved_type(body: &str) -> bool {
    errors_in(body).iter().any(|e| e.starts_with("unresolved-type"))
}

// ── The miss ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_nested_type_that_does_not_exist_is_flagged() {
    assert!(
        flags_unresolved_type("        Object o = new Cfg.MyProva();"),
        "{:?}",
        errors_in("        Object o = new Cfg.MyProva();")
    );
}

/// In a declaration, not only in a `new` — the check is about the NAME, wherever it is written.
#[test]
fn it_is_flagged_in_a_declared_type_too() {
    assert!(flags_unresolved_type("        Cfg.MyProva x = null;"));
}

// ── The other direction ──────────────────────────────────────────────────────────────────────

#[test]
fn a_nested_type_that_exists_is_silent() {
    assert!(errors_in("        Object o = new Cfg.Inner();").is_empty());
    assert!(errors_in("        Cfg.Inner x = null;").is_empty());
}

/// A member type INHERITED from a supertype is named through the subclass too (JLS §8.1.5).
#[test]
fn a_member_type_inherited_from_a_supertype_is_silent() {
    assert!(errors_in("        Cfg.Shared x = null;").is_empty());
}

/// `com.acme.Foo` — neither `com` nor `acme` is a type. Judging it would report a package, and a
/// name somebody spelled out in full is not second-guessed.
#[test]
fn a_package_qualified_name_is_left_alone() {
    assert!(errors_in("        java.util.List<String> l = null;").is_empty());
    assert!(errors_in("        java.util.NopeAtAll x = null;").is_empty());
}

/// A LIBRARY nested type is spelled `Outer$Inner` in bytecode, and asking the classpath about the
/// source spelling is how a check invents a finding. The qualifier has to be a project type.
#[test]
fn a_library_nested_type_is_left_alone() {
    assert!(errors_in("        java.util.Map.Entry<String, String> e = null;").is_empty());
    assert!(errors_in("        Thread.State s = null;").is_empty());
}

/// A type the index has never seen has no nested types to know about either.
#[test]
fn an_unresolvable_qualifier_is_silent() {
    assert!(errors_in("        Unknown.Whatever x = null;").is_empty());
}
