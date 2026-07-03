//! Category: FIELDS — go-to-declaration over field references in every shape the resolver must
//! handle: a bare field use, `this.field`, a field via another object receiver, single- and
//! two-level inherited fields (which resolve into the DECLARING parent's file), an interface
//! constant, a static field, and a field shadowed by a local (the bare use must land on the
//! local, not the field). Each test builds its own tiny, valid Java project and asserts the
//! resolved file + label against the rules the harness guarantees.

mod common;
use common::*;

// ---------------------------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------------------------

/// A single-file project: one class with a field used bare, via `this`, and shadowed by a local.
fn single() -> Project {
    Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   int count;\n\
         \x20   int mirror;\n\
         \x20   int readBare() { return count + 1; }\n\
         \x20   void setThis(int v) { this.count = v; }\n\
         \x20   int shadowed() { int count = 5; return count + mirror; }\n\
         }\n",
    )])
}

/// A three-level inheritance chain: `Grand` → `Parent` → `Child`, each contributing a field.
fn chain() -> Project {
    Project::new(&[
        (
            "Grand.java",
            "package app;\n\
             public class Grand {\n\
             \x20   protected int grandField;\n\
             }\n",
        ),
        (
            "Parent.java",
            "package app;\n\
             public class Parent extends Grand {\n\
             \x20   protected int parentField;\n\
             }\n",
        ),
        (
            "Child.java",
            "package app;\n\
             public class Child extends Parent {\n\
             \x20   int own;\n\
             \x20   int sum() { return own + parentField + grandField; }\n\
             }\n",
        ),
    ])
}

// ---------------------------------------------------------------------------------------------
// Bare / this / receiver
// ---------------------------------------------------------------------------------------------

#[test]
fn bare_field_reference() {
    let p = single();
    let s = p.source("A.java").to_string();
    // `count + 1` in readBare() — a bare reference to the field.
    let d = p.goto("A.java", at(&s, "count + 1")).expect("goto bare field");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "field app.A.count");
    assert_eq!(d.line, line_of(&s, "int count;"));
}

#[test]
fn this_field_reference() {
    let p = single();
    let s = p.source("A.java").to_string();
    // `this.count = v` — the field via the `this` receiver.
    let off = at(&s, "this.count") + "this.".len();
    let d = p.goto("A.java", off).expect("goto this.field");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "field app.A.count");
    assert_eq!(d.line, line_of(&s, "int count;"));
}

#[test]
fn field_via_object_receiver_cross_file() {
    let p = Project::new(&[
        (
            "Holder.java",
            "package app;\n\
             public class Holder {\n\
             \x20   public int value;\n\
             }\n",
        ),
        (
            "User.java",
            "package app;\n\
             public class User {\n\
             \x20   int read(Holder h) { return h.value; }\n\
             }\n",
        ),
    ]);
    let u = p.source("User.java").to_string();
    // `h.value` — a field accessed through another object's receiver, in a different file.
    let off = at(&u, "h.value") + "h.".len();
    let d = p.goto("User.java", off).expect("goto receiver field");
    assert_eq!(d.file, "Holder.java", "field resolves into the owning type's file");
    assert_eq!(d.label, "field app.Holder.value");
    assert_eq!(d.line, line_of(p.source("Holder.java"), "int value;"));
}

// ---------------------------------------------------------------------------------------------
// Inheritance
// ---------------------------------------------------------------------------------------------

#[test]
fn field_inherited_one_level() {
    let p = chain();
    let c = p.source("Child.java").to_string();
    // `parentField` used bare in Child — declared one level up, in Parent.
    let d = p.goto("Child.java", at(&c, "parentField")).expect("goto inherited (1 level)");
    assert_eq!(d.file, "Parent.java", "an inherited field resolves into the declaring parent's file");
    assert_eq!(d.label, "field app.Parent.parentField");
    assert_eq!(d.line, line_of(p.source("Parent.java"), "int parentField;"));
}

#[test]
fn field_inherited_two_levels() {
    let p = chain();
    let c = p.source("Child.java").to_string();
    // `grandField` used bare in Child — declared two levels up, in Grand.
    let d = p.goto("Child.java", at(&c, "grandField")).expect("goto inherited (2 levels)");
    assert_eq!(d.file, "Grand.java", "a two-level inherited field resolves into the grandparent's file");
    assert_eq!(d.label, "field app.Grand.grandField");
    assert_eq!(d.line, line_of(p.source("Grand.java"), "int grandField;"));
}

#[test]
fn own_field_in_subclass_stays_local() {
    let p = chain();
    let c = p.source("Child.java").to_string();
    // `own` is declared in Child itself — must resolve locally, not into a parent.
    let off = at(&c, "return own") + "return ".len();
    let d = p.goto("Child.java", off).expect("goto own field");
    assert_eq!(d.file, "Child.java");
    assert_eq!(d.label, "field app.Child.own");
    assert_eq!(d.line, line_of(&c, "int own;"));
}

// ---------------------------------------------------------------------------------------------
// Interface constant
// ---------------------------------------------------------------------------------------------

#[test]
fn constant_from_implemented_interface() {
    let p = Project::new(&[
        (
            "Limits.java",
            "package app;\n\
             public interface Limits {\n\
             \x20   int MAX = 100;\n\
             }\n",
        ),
        (
            "Bounded.java",
            "package app;\n\
             public class Bounded implements Limits {\n\
             \x20   int cap() { return MAX; }\n\
             }\n",
        ),
    ]);
    let b = p.source("Bounded.java").to_string();
    // `MAX` — an implicitly `public static final` constant inherited from the interface.
    let d = p.goto("Bounded.java", at(&b, "return MAX") + "return ".len())
        .expect("goto interface constant");
    assert_eq!(d.file, "Limits.java", "an interface constant resolves into the interface's file");
    assert_eq!(d.label, "field app.Limits.MAX");
    assert_eq!(d.line, line_of(p.source("Limits.java"), "int MAX ="));
}

#[test]
fn constant_via_interface_type_qualifier() {
    let p = Project::new(&[
        (
            "Config.java",
            "package app;\n\
             public interface Config {\n\
             \x20   int TIMEOUT = 30;\n\
             }\n",
        ),
        (
            "Runner.java",
            "package app;\n\
             public class Runner {\n\
             \x20   int t() { return Config.TIMEOUT; }\n\
             }\n",
        ),
    ]);
    let r = p.source("Runner.java").to_string();
    // `Config.TIMEOUT` — qualified by the interface type from a non-implementing class.
    let off = at(&r, "Config.TIMEOUT") + "Config.".len();
    let d = p.goto("Runner.java", off).expect("goto qualified interface constant");
    assert_eq!(d.file, "Config.java");
    assert_eq!(d.label, "field app.Config.TIMEOUT");
    assert_eq!(d.line, line_of(p.source("Config.java"), "int TIMEOUT ="));
}

// ---------------------------------------------------------------------------------------------
// Static fields
// ---------------------------------------------------------------------------------------------

#[test]
fn static_field_bare_same_class() {
    let p = Project::new(&[(
        "Counter.java",
        "package app;\n\
         public class Counter {\n\
         \x20   static int total;\n\
         \x20   int bump() { return total + 1; }\n\
         }\n",
    )]);
    let s = p.source("Counter.java").to_string();
    // bare `total` in the same class — a static field.
    let d = p.goto("Counter.java", at(&s, "total + 1")).expect("goto static field");
    assert_eq!(d.file, "Counter.java");
    assert_eq!(d.label, "field app.Counter.total");
    assert_eq!(d.line, line_of(&s, "static int total;"));
}

#[test]
fn static_field_via_type_qualifier_cross_file() {
    let p = Project::new(&[
        (
            "Registry.java",
            "package app;\n\
             public class Registry {\n\
             \x20   static int size;\n\
             }\n",
        ),
        (
            "Probe.java",
            "package app;\n\
             public class Probe {\n\
             \x20   int look() { return Registry.size; }\n\
             }\n",
        ),
    ]);
    let pr = p.source("Probe.java").to_string();
    // `Registry.size` — static field qualified by the class name, cross-file.
    let off = at(&pr, "Registry.size") + "Registry.".len();
    let d = p.goto("Probe.java", off).expect("goto qualified static field");
    assert_eq!(d.file, "Registry.java");
    assert_eq!(d.label, "field app.Registry.size");
    assert_eq!(d.line, line_of(p.source("Registry.java"), "static int size;"));
}

#[test]
fn inherited_static_field() {
    let p = Project::new(&[
        (
            "Origin.java",
            "package app;\n\
             public class Origin {\n\
             \x20   static int seed;\n\
             }\n",
        ),
        (
            "Derived.java",
            "package app;\n\
             public class Derived extends Origin {\n\
             \x20   int next() { return seed + 1; }\n\
             }\n",
        ),
    ]);
    let d2 = p.source("Derived.java").to_string();
    // bare `seed` in the subclass — a static field inherited from the parent.
    let off = at(&d2, "return seed") + "return ".len();
    let d = p.goto("Derived.java", off).expect("goto inherited static field");
    assert_eq!(d.file, "Origin.java", "an inherited static field resolves into the declaring parent's file");
    assert_eq!(d.label, "field app.Origin.seed");
    assert_eq!(d.line, line_of(p.source("Origin.java"), "static int seed;"));
}

// ---------------------------------------------------------------------------------------------
// Shadowing
// ---------------------------------------------------------------------------------------------

#[test]
fn field_shadowed_by_local_resolves_to_local() {
    let p = single();
    let s = p.source("A.java").to_string();
    // In shadowed(): `int count = 5; return count + mirror;` — the bare `count` here is the
    // LOCAL, not the field, so it must resolve to the local declaration in this same file.
    let off = at(&s, "return count + mirror") + "return ".len();
    let d = p.goto("A.java", off).expect("goto shadowed name -> local");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `count`", "a local shadows the same-named field for a bare use");
    assert_eq!(d.line, line_of(&s, "int count = 5;"));
}

#[test]
fn shadowing_local_decl_is_a_local() {
    let p = single();
    let s = p.source("A.java").to_string();
    // Caret on the local's own declaration `int count = 5` — it is a local, not the field.
    let off = at(&s, "int count = 5") + "int ".len();
    let d = p.goto("A.java", off).expect("goto local decl");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `count`");
}

#[test]
fn non_shadowed_field_in_shadowing_method_is_field() {
    let p = single();
    let s = p.source("A.java").to_string();
    // In the same `shadowed()` body, `mirror` is NOT shadowed by any local, so the bare use
    // must resolve to the field (proving the local only shadows its own name).
    let off = at(&s, "count + mirror") + "count + ".len();
    let d = p.goto("A.java", off).expect("goto non-shadowed field");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "field app.A.mirror");
    assert_eq!(d.line, line_of(&s, "int mirror;"));
}

// ---------------------------------------------------------------------------------------------
// Field declaration itself + find-usages
// ---------------------------------------------------------------------------------------------

#[test]
fn caret_on_field_declaration() {
    let p = single();
    let s = p.source("A.java").to_string();
    // Caret on the field's own declaration name resolves to itself.
    let off = at(&s, "int count;") + "int ".len();
    let d = p.goto("A.java", off).expect("goto field decl");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "field app.A.count");
    assert_eq!(d.line, line_of(&s, "int count;"));
}

#[test]
fn find_usages_of_inherited_field() {
    let p = chain();
    let pr = p.source("Parent.java").to_string();
    // Child.sum() reads parentField BARE (no `this.`). Bare field references are not bucketed
    // by the find-usages walk (documented limitation — see find_usages.rs), so this counts 0.
    let off = at(&pr, "int parentField;") + "int ".len();
    let n = p.usage_count("Parent.java", off);
    assert_eq!(n, 0, "bare inherited-field reads are not bucketed by find-usages");
}

#[test]
fn find_usages_of_local_is_zero() {
    let p = single();
    let s = p.source("A.java").to_string();
    // A local is scope-exact and not bucketed → usage_count is 0 even though the name recurs.
    let off = at(&s, "int count = 5") + "int ".len();
    assert_eq!(p.usage_count("A.java", off), 0, "a local is not counted by find-usages");
}
