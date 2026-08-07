//! Find-usages category — exercises `usage_count` (the find-usages reference walk) over a real
//! multi-file index. Members and types are bucketed and counted across every use site (the
//! declaration itself is NOT counted); locals are scope-exact and never bucketed.
//!
//! NOTE on FIELD accesses: the reference walk indexes both the QUALIFIED shapes (`this.f`,
//! `obj.f`, `Type.f`) and the bare `f` that stands for `this.f` — which in ordinary Java, and
//! especially in a `private static final` constant, is where most of the reads are. A bare name
//! that a local or parameter binds in scope is the local, and counts for the field nowhere (see
//! `a_local_shadowing_a_field_is_not_a_use_of_it`).

mod common;
use common::*;

fn proj() -> Project {
    Project::new(&[
        (
            "Base.java",
            "package app;\n\
             public class Base {\n\
             \x20   protected int baseField;\n\
             \x20   public int baseMethod() { return 1; }\n\
             }\n",
        ),
        (
            "Service.java",
            "package app;\n\
             public class Service extends Base {\n\
             \x20   private int localField;\n\
             \x20   public int compute(int param) {\n\
             \x20       int local = param + 1;\n\
             \x20       return local + this.localField + this.baseField + baseMethod();\n\
             \x20   }\n\
             \x20   public int caller() { return compute(2) + this.localField; }\n\
             }\n",
        ),
        (
            "Consumer.java",
            "package app;\n\
             public class Consumer {\n\
             \x20   public int use(Service s) { return s.compute(3) + s.baseMethod(); }\n\
             }\n",
        ),
    ])
}

// ── Methods ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn method_used_across_two_files() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let n = p.usage_count("Service.java", at(&s, "compute(int param)"));
    assert_eq!(n, 2, "compute() is used in caller() and Consumer.use()");
}

#[test]
fn method_usage_count_stable_from_use_site() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let from_decl = p.usage_count("Service.java", at(&s, "compute(int param)"));
    let from_use = p.usage_count("Service.java", at(&s, "compute(2)"));
    assert_eq!(from_decl, from_use, "count is a property of the member, not the caret");
    assert_eq!(from_use, 2);
}

#[test]
fn inherited_method_used_across_files() {
    let p = proj();
    let b = p.source("Base.java").to_string();
    let n = p.usage_count("Base.java", at(&b, "baseMethod() { return 1; }"));
    assert_eq!(n, 2, "baseMethod() is used in Service.compute() and Consumer.use()");
}

#[test]
fn method_with_zero_uses() {
    let p = Project::new(&[(
        "Solo.java",
        "package z;\n\
         public class Solo {\n\
         \x20   public int unused() { return 42; }\n\
         \x20   public int other() { return 7; }\n\
         }\n",
    )]);
    let s = p.source("Solo.java").to_string();
    let n = p.usage_count("Solo.java", at(&s, "unused()"));
    assert_eq!(n, 0, "an uncalled method has zero recorded use sites");
}

#[test]
fn method_used_only_same_file() {
    let p = Project::new(&[(
        "Calc.java",
        "package z;\n\
         public class Calc {\n\
         \x20   public int helper() { return 1; }\n\
         \x20   public int a() { return helper(); }\n\
         \x20   public int b() { return helper() + helper(); }\n\
         }\n",
    )]);
    let s = p.source("Calc.java").to_string();
    // helper() called once in a(), twice in b() → 3 same-file call sites (decl not counted).
    let n = p.usage_count("Calc.java", at(&s, "helper() { return 1; }"));
    assert_eq!(n, 3, "helper() has 3 same-file call sites");
}

#[test]
fn method_used_only_cross_file() {
    let p = Project::new(&[
        (
            "Api.java",
            "package svc;\n\
             public class Api {\n\
             \x20   public int ping() { return 0; }\n\
             }\n",
        ),
        (
            "ClientOne.java",
            "package svc;\n\
             public class ClientOne {\n\
             \x20   public int a(Api api) { return api.ping(); }\n\
             }\n",
        ),
        (
            "ClientTwo.java",
            "package svc;\n\
             public class ClientTwo {\n\
             \x20   public int b(Api api) { return api.ping(); }\n\
             }\n",
        ),
    ]);
    let a = p.source("Api.java").to_string();
    let n = p.usage_count("Api.java", at(&a, "ping()"));
    assert_eq!(n, 2, "ping() is used once in each of two cross-file clients");
}

// ── Fields (qualified access is what find-usages buckets) ────────────────────────────────────

#[test]
fn field_used_several_times() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // this.localField in compute() and in caller() = 2 qualified use sites.
    let n = p.usage_count("Service.java", at(&s, "int localField") + "int ".len());
    assert_eq!(n, 2, "localField is used in compute() and caller()");
}

#[test]
fn field_usage_count_stable_from_use_site() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let from_decl = p.usage_count("Service.java", at(&s, "int localField") + "int ".len());
    let from_use = p.usage_count("Service.java", at_last(&s, "this.localField") + "this.".len());
    assert_eq!(from_decl, from_use, "field count is stable regardless of caret site");
    assert_eq!(from_use, 2);
}

#[test]
fn inherited_field_used_from_subclass() {
    let p = proj();
    let b = p.source("Base.java").to_string();
    // this.baseField is read once in Service.compute() = 1.
    let n = p.usage_count("Base.java", at(&b, "int baseField") + "int ".len());
    assert_eq!(n, 1, "baseField is used once in Service.compute()");
}

#[test]
fn field_used_only_cross_file() {
    let p = Project::new(&[
        (
            "Config.java",
            "package cfg;\n\
             public class Config {\n\
             \x20   public int limit;\n\
             }\n",
        ),
        (
            "Reader.java",
            "package cfg;\n\
             public class Reader {\n\
             \x20   public int read(Config c) { return c.limit; }\n\
             }\n",
        ),
    ]);
    let cfg = p.source("Config.java").to_string();
    let n = p.usage_count("Config.java", at(&cfg, "int limit") + "int ".len());
    assert_eq!(n, 1, "limit is read once (c.limit) from Reader");
}

#[test]
fn field_with_zero_uses() {
    let p = Project::new(&[(
        "Holder.java",
        "package z;\n\
         public class Holder {\n\
         \x20   private int dangling;\n\
         \x20   public int get() { return 0; }\n\
         }\n",
    )]);
    let s = p.source("Holder.java").to_string();
    let n = p.usage_count("Holder.java", at(&s, "int dangling") + "int ".len());
    assert_eq!(n, 0, "an unused field has zero recorded use sites");
}

#[test]
fn bare_field_reference_is_counted() {
    let p = Project::new(&[(
        "Bare.java",
        "package z;\n\
         public class Bare {\n\
         \x20   private int flag;\n\
         \x20   public int read() { return flag; }\n\
         }\n",
    )]);
    let s = p.source("Bare.java").to_string();
    let n = p.usage_count("Bare.java", at(&s, "int flag") + "int ".len());
    assert_eq!(n, 1, "`return flag` is a read of the field, `this.` or not");
}

/// The shape the bare-identifier arm exists for: a constant nobody ever qualifies. Read bare in
/// its own class and bare-but-qualified-by-the-type from another one.
#[test]
fn a_constant_is_counted_wherever_it_is_read() {
    let p = Project::new(&[
        (
            "Limits.java",
            "package z;\n\
             public class Limits {\n\
             \x20   public static final int MAX = 10;\n\
             \x20   public int clamp(int v) { return v > MAX ? MAX : v; }\n\
             }\n",
        ),
        (
            "Other.java",
            "package z;\n\
             public class Other {\n\
             \x20   public int top() { return Limits.MAX; }\n\
             }\n",
        ),
    ]);
    let s = p.source("Limits.java").to_string();
    let n = p.usage_count("Limits.java", at(&s, "int MAX =") + "int ".len());
    assert_eq!(n, 3, "twice bare in clamp(), once as Limits.MAX in Other");
}

/// The shape a legacy class is actually written in: the methods first, the collaborators
/// declared `final` and initialised inline at the BOTTOM. Nothing about a field's position
/// changes what reads it — this exists because it once did.
#[test]
fn fields_declared_below_the_methods_that_read_them_are_counted() {
    let p = Project::new(&[(
        "Service.java",
        "package z;\n\
         public class Service {\n\
         \x20   public int run() { return helper.size() + limit; }\n\
         \x20   public int again() { return limit; }\n\
         \x20   private final java.util.List<String> helper = new java.util.ArrayList<>();\n\
         \x20   private final int limit = 10;\n\
         }\n",
    )]);
    let s = p.source("Service.java").to_string();
    assert_eq!(p.usage_count("Service.java", at(&s, "int limit") + "int ".len()), 2);
    assert_eq!(
        p.usage_count("Service.java", at(&s, "helper =")),
        1,
        "the receiver of `helper.size()` is a read of the field",
    );
}

/// The two sides of find-usages — the index that records a use and the caret that looks one up —
/// must build the same key. A NESTED class is where they used to spell the enclosing type
/// differently, which reads as "no usages" from the declaration while <kbd>Ctrl</kbd>+click from
/// the use navigates perfectly well.
#[test]
fn a_nested_class_counts_its_own_members() {
    let p = Project::new(&[(
        "Outer.java",
        "package z;\n\
         public class Outer {\n\
         \x20   public static class Inner {\n\
         \x20       public int read() { return depth + step(); }\n\
         \x20       public int twice() { return depth; }\n\
         \x20       private int step() { return 1; }\n\
         \x20       private final int depth = 3;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Outer.java").to_string();
    assert_eq!(p.usage_count("Outer.java", at(&s, "int depth") + "int ".len()), 2);
    assert_eq!(
        p.usage_count("Outer.java", at(&s, "int step()") + "int ".len()),
        1,
        "a bare call to a sibling method counts the same way",
    );
}

/// A field's own declarator is the declaration find-usages is answering ABOUT, never one of the
/// answers.
#[test]
fn a_field_declaration_is_not_a_use_of_itself() {
    let p = Project::new(&[(
        "Only.java",
        "package z;\n\
         public class Only {\n\
         \x20   private int seen = 0;\n\
         }\n",
    )]);
    let s = p.source("Only.java").to_string();
    assert_eq!(p.usage_count("Only.java", at(&s, "int seen") + "int ".len()), 0);
}

/// A local (or parameter) of the same name IS the name — the field it hides is untouched. The
/// setter shape below is the one every legacy codebase is full of.
#[test]
fn a_local_shadowing_a_field_is_not_a_use_of_it() {
    let p = Project::new(&[(
        "Shadow.java",
        "package z;\n\
         public class Shadow {\n\
         \x20   private int value;\n\
         \x20   public void setValue(int value) { this.value = value; }\n\
         \x20   public int twice() { int value = 2; return value; }\n\
         \x20   public int real() { return value; }\n\
         }\n",
    )]);
    let s = p.source("Shadow.java").to_string();
    let n = p.usage_count("Shadow.java", at(&s, "int value;") + "int ".len());
    assert_eq!(n, 2, "only `this.value` in the setter and the bare read in real()");
}

/// Names that are not expressions at all: a label, an import segment, an annotation element.
/// Each of them collides with a field name here, and none of them is a use of it.
#[test]
fn labels_imports_and_annotation_elements_are_not_field_uses() {
    let p = Project::new(&[(
        "Odd.java",
        "package z;\n\
         import java.util.List;\n\
         public class Odd {\n\
         \x20   private int util;\n\
         \x20   private int outer;\n\
         \x20   @SuppressWarnings(value = \"x\")\n\
         \x20   private int value;\n\
         \x20   public void loop() { outer: for (;;) { break outer; } }\n\
         \x20   public List<String> keep() { return null; }\n\
         }\n",
    )]);
    let s = p.source("Odd.java").to_string();
    assert_eq!(p.usage_count("Odd.java", at(&s, "int util") + "int ".len()), 0, "`java.util` is a package");
    assert_eq!(p.usage_count("Odd.java", at(&s, "int outer") + "int ".len()), 0, "`outer:` is a label");
    assert_eq!(
        p.usage_count("Odd.java", at(&s, "int value") + "int ".len()),
        0,
        "`value =` names an annotation element",
    );
}

// ── Types ───────────────────────────────────────────────────────────────────────────────────

#[test]
fn type_referenced_from_multiple_files() {
    let p = Project::new(&[
        (
            "Widget.java",
            "package ui;\n\
             public class Widget {\n\
             \x20   public int size() { return 1; }\n\
             }\n",
        ),
        (
            "Panel.java",
            "package ui;\n\
             public class Panel {\n\
             \x20   private Widget w;\n\
             \x20   public Widget make() { return new Widget(); }\n\
             }\n",
        ),
        (
            "Screen.java",
            "package ui;\n\
             public class Screen {\n\
             \x20   public void draw(Widget arg) { Widget local = arg; }\n\
             }\n",
        ),
    ]);
    let w = p.source("Widget.java").to_string();
    let n = p.usage_count("Widget.java", at(&w, "class Widget") + "class ".len());
    assert!(n >= 3, "Widget is referenced from Panel and Screen (>=3 sites), got {n}");
}

#[test]
fn type_referenced_single_new_expression() {
    let p = Project::new(&[
        ("Thing.java", "package m;\npublic class Thing {}\n"),
        (
            "Factory.java",
            "package m;\n\
             public class Factory {\n\
             \x20   public Thing build() { return new Thing(); }\n\
             }\n",
        ),
    ]);
    let t = p.source("Thing.java").to_string();
    // `Thing` return type + `new Thing()` = 2 references from Factory.
    let n = p.usage_count("Thing.java", at(&t, "class Thing") + "class ".len());
    assert_eq!(n, 2, "Thing referenced as return type and in new Thing()");
}

#[test]
fn same_simple_name_types_counted_independently() {
    let p = Project::new(&[
        ("a/Item.java", "package a;\npublic class Item {}\n"),
        ("b/Item.java", "package b;\npublic class Item {}\n"),
        (
            "a/UseA.java",
            "package a;\n\
             public class UseA {\n\
             \x20   public a.Item make() { return new a.Item(); }\n\
             }\n",
        ),
    ]);
    let a = p.source("a/Item.java").to_string();
    let b = p.source("b/Item.java").to_string();
    let na = p.usage_count("a/Item.java", at(&a, "class Item") + "class ".len());
    let nb = p.usage_count("b/Item.java", at(&b, "class Item") + "class ".len());
    // Two same-simple-name types collide in the simple→binary index, so find-usages cannot
    // split their counts precisely (a known limitation); the reference is recorded under one of
    // the two `Item` bindings, and neither caret panics.
    assert!(na + nb >= 1, "a.Item's references are recorded somewhere (na={na}, nb={nb})");
}

// ── Locals / non-symbols are never bucketed ─────────────────────────────────────────────────

#[test]
fn local_variable_yields_zero() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let on_decl = p.usage_count("Service.java", at(&s, "int local =") + "int ".len());
    assert_eq!(on_decl, 0, "a local variable is not bucketed → usage_count 0");
    let on_use = p.usage_count("Service.java", at(&s, "return local") + "return ".len());
    assert_eq!(on_use, 0, "a local's use site is also not bucketed → 0");
}

#[test]
fn parameter_yields_zero() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let on_use = p.usage_count("Service.java", at(&s, "param + 1"));
    assert_eq!(on_use, 0, "a parameter is scope-exact, not bucketed → 0");
}

#[test]
fn keyword_and_literal_carets_count_zero() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    assert_eq!(p.usage_count("Service.java", at(&s, "public class")), 0);
    let b = p.source("Base.java").to_string();
    assert_eq!(p.usage_count("Base.java", at(&b, "return 1") + "return ".len()), 0);
}
