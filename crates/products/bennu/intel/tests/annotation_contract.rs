//! What an annotation type promises, and what a use site owes it — end to end over the real index.
//!
//! The unit tests for these checks feed a hand-written member list, which is the right place to pin
//! the RULES and the wrong place to prove they hold: the answer depends on a `default` clause
//! surviving the source parse, the persisted index and the resolver, and a fixture asserts none of
//! that. Every case here was first run through `javac 21` and matches what it does.

mod common;

use common::Project;

/// An annotation with one required element and one with a default — the shape of every
/// configuration annotation in a legacy codebase.
const TAG: &str = r#"package com.acme;
public @interface Tag {
    String name();
    int count() default 3;
}
"#;

/// A single-element annotation, so the `@Only("x")` shorthand is legal on it.
const ONLY: &str = r#"package com.acme;
public @interface Only { String value(); }
"#;

const HOLDER: &str = r#"package com.acme;
public class Holder {
    public static String MUTABLE = "m";
    public static final String CONST = "c";
    public static final Thing[] THINGS = new Thing[0];
    public static class Thing {}
}
"#;

/// The error codes on `file`, which for a compiling shape must be empty.
fn codes(files: &[(&str, &str)], file: &str) -> Vec<String> {
    Project::new(files)
        .validate(file)
        .into_iter()
        .filter(|d| d.severity == "error")
        // The harness names files by package path, not by a real source root, so the
        // public-type/file-name check has nothing to say here.
        .filter(|d| d.code != "type-name-mismatch-file")
        .map(|d| d.code)
        .collect()
}

fn tag_case(name: &str, body: &str) -> Vec<String> {
    let path = format!("com/acme/{name}.java");
    let src = format!("package com.acme;\npublic class {name} {{ {body} }}\n");
    codes(&[("com/acme/Tag.java", TAG), (&path, &src)], &path)
}

// ── an element that has to be supplied ───────────────────────────────────────

#[test]
fn a_required_element_left_out_is_flagged() {
    assert_eq!(tag_case("R1", r#"@Tag(count = 1) void m() {}"#), ["missing-annotation-element"]);
}

#[test]
fn a_marker_use_of_an_annotation_with_a_required_element_is_flagged() {
    assert_eq!(tag_case("R2", r#"@Tag void m() {}"#), ["missing-annotation-element"]);
}

#[test]
fn supplying_the_required_element_is_enough() {
    assert!(tag_case("R3", r#"@Tag(name = "a") void m() {}"#).is_empty());
    assert!(tag_case("R4", r#"@Tag(name = "a", count = 1) void m() {}"#).is_empty());
}

/// The `default` clause has to survive the source parse and the persisted index, or `count` would
/// read as required and every use of `@Tag` in the project would be reported.
#[test]
fn a_defaulted_element_is_never_required() {
    assert!(!tag_case("R5", r#"@Tag(name = "a") void m() {}"#)
        .contains(&"missing-annotation-element".to_string()));
}

// ── the single-value shorthand ───────────────────────────────────────────────

#[test]
fn the_shorthand_is_fine_on_a_type_that_declares_value() {
    let src = "package com.acme;\npublic class S2 { @Only(\"x\") void m() {} }\n";
    assert!(codes(
        &[("com/acme/Only.java", ONLY), ("com/acme/S2.java", src)],
        "com/acme/S2.java"
    )
    .is_empty());
}

/// javac reports two things here — a `value()` it cannot find, and the `name` the shorthand did not
/// supply — and both are worth saying: one names the mistake, the other names the fix.
#[test]
fn the_shorthand_on_a_type_without_value_is_flagged() {
    let mut got = tag_case("S3", r#"@Tag("x") void m() {}"#);
    got.sort();
    assert_eq!(got, ["missing-annotation-element", "unknown-annotation-element"]);
}

// ── a value that is not a constant ───────────────────────────────────────────

fn holder_case(name: &str, body: &str) -> Vec<String> {
    let path = format!("com/acme/{name}.java");
    let src = format!("package com.acme;\npublic class {name} {{ {body} }}\n");
    codes(
        &[("com/acme/Tag.java", TAG), ("com/acme/Holder.java", HOLDER), (&path, &src)],
        &path,
    )
}

#[test]
fn a_non_final_field_as_a_value_is_flagged() {
    assert_eq!(
        holder_case("C1", r#"@Tag(name = Holder.MUTABLE) void m() {}"#),
        ["non-constant-annotation-value"]
    );
}

#[test]
fn a_static_final_string_as_a_value_is_fine() {
    assert!(holder_case("C2", r#"@Tag(name = Holder.CONST) void m() {}"#).is_empty());
}

/// `final` is necessary and not sufficient (JLS §4.12.4) — a constant variable is also of a
/// primitive or `String` type.
#[test]
fn a_final_field_of_a_class_type_as_a_value_is_flagged() {
    assert_eq!(
        holder_case("C3", r#"@Tag(name = Holder.THINGS) void m() {}"#),
        ["non-constant-annotation-value"]
    );
}

/// A field of the class the annotation is written in, named without a qualifier.
#[test]
fn an_unqualified_non_final_field_is_flagged() {
    assert_eq!(
        tag_case("C4", r#"static String N = "n"; @Tag(name = N) void m() {}"#),
        ["non-constant-annotation-value"]
    );
}

/// javac accepts an INSTANCE `final String` with a constant initializer just as readily as a static
/// one — the rule is about `final` plus the type, not about `static`.
#[test]
fn a_final_instance_field_as_a_value_is_fine() {
    assert!(tag_case("C5", r#"final String N = "n"; @Tag(name = N) void m() {}"#).is_empty());
}

/// Reading out of an array is never constant, even out of a `static final` one.
#[test]
fn an_array_access_as_a_value_is_flagged() {
    assert_eq!(
        tag_case("C6", r#"static final String[] A = {"n"}; @Tag(name = A[0]) void m() {}"#),
        ["non-constant-annotation-value"]
    );
}

/// Constant folding is javac's, not ours: `"a" + "b"` and `1 + 2` are constant expressions and must
/// pass untouched.
#[test]
fn folded_constants_are_fine() {
    assert!(tag_case("C7", r#"@Tag(name = "a" + "b", count = 1 + 2) void m() {}"#).is_empty());
}

/// A name inside a method body may be a local shadowing the field — and then the field's own
/// modifiers say nothing about what the name means, so nothing is said.
#[test]
fn a_name_shadowed_by_a_local_is_left_alone() {
    assert!(tag_case(
        "C8",
        r#"static String N = "x"; void m() { final String N = "y"; @Tag(name = N) int i = 0; }"#
    )
    .is_empty());
}

// ── what an `@interface` may declare ─────────────────────────────────────────

const KINDS: &str = r#"package com.acme;
public class Kinds {
    public static class MyObj {}
    public enum Color { RED, BLUE }
}
"#;

fn decl_case(name: &str, elements: &str) -> Vec<String> {
    let path = format!("com/acme/{name}.java");
    let src = format!("package com.acme;\npublic @interface {name} {{ {elements} }}\n");
    codes(
        &[("com/acme/Kinds.java", KINDS), ("com/acme/Tag.java", TAG), (&path, &src)],
        &path,
    )
}

/// The whole legal set (JLS §9.6.1) in one declaration.
#[test]
fn every_legal_element_type_is_fine() {
    assert!(decl_case(
        "D1",
        "Kinds.Color c(); Class<?> k(); Class<? extends Number> k2(); Tag t(); String[] a(); int[] i(); String s();"
    )
    .is_empty());
}

/// The report this comes from: `MyObj[]` reads fine as an element type, and then every use of the
/// element looks like a bad VALUE — `OBJ` is `final`, so why is it refused? — when the declaration
/// is what is wrong.
#[test]
fn a_class_typed_element_is_flagged() {
    assert_eq!(decl_case("D2", "Kinds.MyObj o();"), ["invalid-annotation-element-type"]);
    assert_eq!(decl_case("D3", "Kinds.MyObj[] o();"), ["invalid-annotation-element-type"]);
    assert_eq!(decl_case("D4", "Object o();"), ["invalid-annotation-element-type"]);
}

#[test]
fn a_two_dimensional_array_element_is_flagged() {
    assert_eq!(decl_case("D5", "String[][] a();"), ["invalid-annotation-element-type"]);
}

#[test]
fn a_void_element_is_flagged() {
    assert_eq!(decl_case("D6", "void v();"), ["invalid-annotation-element-type"]);
}
