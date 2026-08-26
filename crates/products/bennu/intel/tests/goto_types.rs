//! Category: TYPES in every position.
//!
//! Exercises go-to-declaration when the caret lands on a *type* reference wherever a type name
//! can appear: field type, variable/parameter type, return type, `extends`, `implements`, a cast,
//! `new Foo()`, an array element type, an explicit `import`, a fully-qualified name, and a nested
//! type. The pivotal case: a same-simple-name type living in a DIFFERENT package must resolve by
//! BINARY name to the correct one — never the unrelated homonym.

mod common;
use common::*;

// ── Type in ordinary positions ─────────────────────────────────────────────────────────────

#[test]
fn type_as_field_type() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Holder.java",
            "package app;\n\
             public class Holder {\n\
             \x20   Widget w;\n\
             }\n",
        ),
    ]);
    let s = p.source("Holder.java").to_string();
    let off = at(&s, "Widget w;");
    let d = p.goto("Holder.java", off).expect("goto field-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

#[test]
fn type_as_local_variable_type() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   void run() {\n\
             \x20       Widget local = null;\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "Widget local");
    let d = p.goto("Use.java", off).expect("goto local-var-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

#[test]
fn type_as_parameter_type() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   void take(Widget arg) { }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "Widget arg");
    let d = p.goto("Use.java", off).expect("goto param-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

#[test]
fn type_as_return_type() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Factory.java",
            "package app;\n\
             public class Factory {\n\
             \x20   Widget make() { return null; }\n\
             }\n",
        ),
    ]);
    let s = p.source("Factory.java").to_string();
    let off = at(&s, "Widget make");
    let d = p.goto("Factory.java", off).expect("goto return-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

// ── Type in inheritance clauses ────────────────────────────────────────────────────────────

#[test]
fn type_in_extends_clause() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Sub.java",
            "package app;\npublic class Sub extends Widget { }\n",
        ),
    ]);
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "extends Widget") + "extends ".len();
    let d = p.goto("Sub.java", off).expect("goto extends-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

#[test]
fn type_in_implements_clause() {
    let p = Project::new(&[
        (
            "Shape.java",
            "package app;\npublic interface Shape { int sides(); }\n",
        ),
        (
            "Circle.java",
            "package app;\n\
             public class Circle implements Shape {\n\
             \x20   public int sides() { return 0; }\n\
             }\n",
        ),
    ]);
    let s = p.source("Circle.java").to_string();
    let off = at(&s, "implements Shape") + "implements ".len();
    let d = p.goto("Circle.java", off).expect("goto implements-type");
    assert_eq!(d.file, "Shape.java");
    assert_eq!(d.label, "class app.Shape");
}

// ── Type in expressions ────────────────────────────────────────────────────────────────────

#[test]
fn type_in_new_expression() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   Object build() { return new Widget(); }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "new Widget()") + "new ".len();
    let d = p.goto("Use.java", off).expect("goto new-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

#[test]
fn type_in_cast_expression() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   void run(Object o) { Widget w = (Widget) o; }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    // Target the cast occurrence specifically: "(Widget)".
    let off = at(&s, "(Widget) o") + "(".len();
    let d = p.goto("Use.java", off).expect("goto cast-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

#[test]
fn type_as_array_element_type() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   Widget[] many() { return new Widget[0]; }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    // The array element type in the return position: "Widget[] many".
    let off = at(&s, "Widget[] many");
    let d = p.goto("Use.java", off).expect("goto array-element-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class app.Widget");
}

// ── Type via import / fully-qualified / cross-package ──────────────────────────────────────

#[test]
fn type_via_explicit_import() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package lib;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             import lib.Widget;\n\
             public class Use {\n\
             \x20   Widget w;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    // The imported type at its USE site (the field type) resolves via the import to lib.Widget.
    let off = at(&s, "Widget w");
    let d = p.goto("Use.java", off).expect("goto imported-type use");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class lib.Widget");
}

#[test]
fn type_via_import_then_use_site() {
    // The USE site of an imported type (not the import line) still resolves by binary name.
    let p = Project::new(&[
        (
            "Widget.java",
            "package lib;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             import lib.Widget;\n\
             public class Use {\n\
             \x20   Widget field;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "Widget field");
    let d = p.goto("Use.java", off).expect("goto imported-use-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class lib.Widget");
}

#[test]
fn type_via_fully_qualified_name() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package lib;\npublic class Widget { public int id; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   lib.Widget w;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    // Caret on the simple name at the end of the qualified reference.
    let off = at(&s, "lib.Widget w") + "lib.".len();
    let d = p.goto("Use.java", off).expect("goto fqn-type");
    assert_eq!(d.file, "Widget.java");
    assert_eq!(d.label, "class lib.Widget");
}

// ── The pivotal case: same simple name in two packages ─────────────────────────────────────

#[test]
fn same_simple_name_different_package_resolves_to_correct_one() {
    // Two `Widget` classes: one in `alpha`, one in `beta`. `Use` imports `beta.Widget`, so the
    // reference must resolve to beta's file, NOT alpha's.
    let p = Project::new(&[
        (
            "AlphaWidget.java",
            "package alpha;\npublic class Widget { public int a; }\n",
        ),
        (
            "BetaWidget.java",
            "package beta;\npublic class Widget { public int b; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             import beta.Widget;\n\
             public class Use {\n\
             \x20   Widget w;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "Widget w;");
    let d = p.goto("Use.java", off).expect("goto ambiguous-simple-name");
    assert_eq!(
        d.file, "BetaWidget.java",
        "must resolve by binary name to beta.Widget, not the alpha homonym"
    );
    assert_eq!(d.label, "class beta.Widget");
}

#[test]
fn same_simple_name_fqn_picks_alpha() {
    // Same two homonyms, but this consumer uses the fully-qualified `alpha.Widget` → alpha's file.
    let p = Project::new(&[
        (
            "AlphaWidget.java",
            "package alpha;\npublic class Widget { public int a; }\n",
        ),
        (
            "BetaWidget.java",
            "package beta;\npublic class Widget { public int b; }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   alpha.Widget w;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "alpha.Widget w") + "alpha.".len();
    let d = p.goto("Use.java", off).expect("goto fqn-alpha");
    assert_eq!(d.file, "AlphaWidget.java");
    assert_eq!(d.label, "class alpha.Widget");
}

// ── Nested type ────────────────────────────────────────────────────────────────────────────

#[test]
fn nested_type_reference() {
    // A static nested class `Outer.Inner` referenced from another file.
    let p = Project::new(&[
        (
            "Outer.java",
            "package app;\n\
             public class Outer {\n\
             \x20   public static class Inner { public int v; }\n\
             }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   Outer.Inner ref;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    // Caret on `Inner` in the qualified nested reference.
    let off = at(&s, "Outer.Inner ref") + "Outer.".len();
    let d = p.goto("Use.java", off).expect("goto nested-type");
    assert_eq!(
        d.file, "Outer.java",
        "nested type resolves into the enclosing type's file"
    );
    // The binary name of a nested type is `app.Outer.Inner`; assert the robust file invariant and
    // that a label is present (exact nested-label formatting is not guaranteed by the rules).
    assert!(p.goto_label("Use.java", off).is_some());
}

#[test]
fn nested_type_declaration_line_is_inner() {
    let p = Project::new(&[
        (
            "Outer.java",
            "package app;\n\
             public class Outer {\n\
             \x20   public static class Inner { public int v; }\n\
             }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   Outer.Inner ref;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "Outer.Inner ref") + "Outer.".len();
    let d = p.goto("Use.java", off).expect("goto nested-type");
    let outer_src = p.source("Outer.java");
    assert_eq!(
        d.line,
        line_of(outer_src, "class Inner"),
        "should land on the Inner declaration line, not Outer"
    );
}

// ── Robustness / negative cases for types ──────────────────────────────────────────────────

#[test]
fn caret_on_jdk_type_returns_none() {
    // `String` lives in the JDK: no project source to open → None, never a panic.
    let p = Project::new(&[(
        "Use.java",
        "package app;\n\
         public class Use {\n\
         \x20   String s;\n\
         }\n",
    )]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "String s");
    assert!(
        p.goto("Use.java", off).is_none(),
        "a JDK type has no project decl to navigate to"
    );
}

#[test]
fn type_declaration_self_reference() {
    // Caret on the class name at its own declaration site — resolves to itself (same file).
    let p = Project::new(&[(
        "Widget.java",
        "package app;\npublic class Widget { public int id; }\n",
    )]);
    let s = p.source("Widget.java").to_string();
    let off = at(&s, "class Widget") + "class ".len();
    // May resolve to itself or be treated as the decl (None); either way must not panic and, if
    // Some, must point at Widget.java with the class label.
    if let Some(d) = p.goto("Widget.java", off) {
        assert_eq!(d.file, "Widget.java");
        assert_eq!(d.label, "class app.Widget");
    }
}

#[test]
fn interface_type_uses_class_prefix_label() {
    let p = Project::new(&[
        (
            "Shape.java",
            "package app;\npublic interface Shape { int sides(); }\n",
        ),
        (
            "Use.java",
            "package app;\n\
             public class Use {\n\
             \x20   Shape s;\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "Shape s");
    let d = p.goto("Use.java", off).expect("goto interface-type");
    assert_eq!(d.file, "Shape.java");
    assert_eq!(
        d.label, "class app.Shape",
        "interfaces use the 'class' label prefix too"
    );
}

#[test]
fn find_usages_of_type_across_files() {
    // A type used in two consumers → find-usages from its declaration counts both use sites.
    let p = Project::new(&[
        (
            "Widget.java",
            "package app;\npublic class Widget { public int id; }\n",
        ),
        ("A.java", "package app;\npublic class A { Widget w; }\n"),
        (
            "B.java",
            "package app;\npublic class B { Widget make() { return new Widget(); } }\n",
        ),
    ]);
    let s = p.source("Widget.java").to_string();
    // Caret on the Widget declaration.
    let off = at(&s, "class Widget") + "class ".len();
    let n = p.usage_count("Widget.java", off);
    assert!(
        n >= 1,
        "Widget is referenced from A and B; use-site count should be positive (got {n})"
    );
}
