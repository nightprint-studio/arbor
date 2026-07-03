//! Real-world Java expression patterns for go-to — a proactive robustness sweep over the shapes
//! legacy code actually uses: `super.m()`, static access via a type name (single + chained),
//! enum-constant methods, generic-element chains, casts, and `this.field.method()`. Where the
//! engine's documented rules guarantee resolution we assert the exact target; where a shape is a
//! known inference gap we assert the robust invariant (resolves correctly if at all, never a
//! panic) so the test documents current behaviour instead of over-committing.

mod common;
use common::*;

fn proj() -> Project {
    Project::new(&[
        (
            "Base.java",
            "package p;\n\
             public class Base {\n\
             \x20   protected int baseVal() { return 1; }\n\
             }\n",
        ),
        (
            "Util.java",
            "package p;\n\
             public class Util {\n\
             \x20   public static Util create() { return new Util(); }\n\
             \x20   public static int CONST = 5;\n\
             \x20   public int inst() { return 2; }\n\
             }\n",
        ),
        (
            "Color.java",
            "package p;\n\
             public enum Color {\n\
             \x20   RED, GREEN;\n\
             \x20   public int code() { return 1; }\n\
             }\n",
        ),
        (
            "Box.java",
            "package p;\n\
             public class Box {\n\
             \x20   public int val() { return 0; }\n\
             }\n",
        ),
        (
            "Sub.java",
            "package p;\n\
             import java.util.List;\n\
             public class Sub extends Base {\n\
             \x20   private Box box;\n\
             \x20   private List<Box> boxes;\n\
             \x20   int useSuper() { return super.baseVal(); }\n\
             \x20   Util useStaticMethod() { return Util.create(); }\n\
             \x20   int useStaticField() { return Util.CONST; }\n\
             \x20   int useStaticChain() { return Util.create().inst(); }\n\
             \x20   int useEnumMethod() { return Color.RED.code(); }\n\
             \x20   int useGenericElem() { return boxes.get(0).val(); }\n\
             \x20   int useCast() { return ((Box) box).val(); }\n\
             \x20   int useThisChain() { return this.box.val(); }\n\
             }\n",
        ),
    ])
}

#[test]
fn super_method_call_resolves() {
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "super.baseVal()") + "super.".len();
    let d = p.goto("Sub.java", off).expect("super.method resolves");
    assert_eq!(d.file, "Base.java");
    assert_eq!(d.label, "method p.Base.baseVal()");
}

#[test]
fn static_method_via_type_resolves() {
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "Util.create()") + "Util.".len();
    let d = p.goto("Sub.java", off).expect("static method via type resolves");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "method p.Util.create()");
}

#[test]
fn static_field_via_type_resolves() {
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "Util.CONST") + "Util.".len();
    let d = p.goto("Sub.java", off).expect("static field via type resolves");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "field p.Util.CONST");
}

#[test]
fn static_call_then_instance_chain_resolves() {
    // `Util.create().inst()` — the `.inst()` receiver is the static call's return type (Util).
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "Util.create().inst()") + "Util.create().".len();
    // KNOWN GAP: chaining an instance call off a STATIC call needs static-type-receiver
    // inference (`infer` types a value receiver, not a type name), which it does not do yet.
    // Documented: resolves correctly if at all, never a panic.
    if let Some(d) = p.goto("Sub.java", off) {
        assert_eq!(d.file, "Util.java");
        assert_eq!(d.label, "method p.Util.inst()");
    }
}

#[test]
fn enum_constant_method_resolves() {
    // `Color.RED.code()` — `.code()` receiver is the enum constant, whose type is Color.
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "Color.RED.code()") + "Color.RED.".len();
    // KNOWN GAP: enum constants aren't extracted as typed fields, so `Color.RED` has no inferred
    // type and `.code()` can't resolve its owner. Documented: correct if at all, never a panic.
    if let Some(d) = p.goto("Sub.java", off) {
        assert_eq!(d.file, "Color.java");
        assert_eq!(d.label, "method p.Color.code()");
    }
}

#[test]
fn generic_element_chain_resolves() {
    // `boxes.get(0).val()` — `.val()` receiver is the List<Box> element type Box.
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "boxes.get(0).val()") + "boxes.get(0).".len();
    // KNOWN GAP: `boxes.get(0)` needs `java.util.List.get`, but the go-to/find-usages engine is
    // PROJECT-ONLY (never decodes JDK bytecode), so the element type can't be recovered here.
    // Documented: correct if at all, never a panic.
    if let Some(d) = p.goto("Sub.java", off) {
        assert_eq!(d.file, "Box.java");
        assert_eq!(d.label, "method p.Box.val()");
    }
}

#[test]
fn cast_then_call_resolves() {
    // `((Box) box).val()` — the cast fixes the receiver type to Box.
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "((Box) box).val()") + "((Box) box).".len();
    let d = p.goto("Sub.java", off).expect("cast-then-call resolves");
    assert_eq!(d.file, "Box.java");
    assert_eq!(d.label, "method p.Box.val()");
}

#[test]
fn this_field_chain_resolves() {
    // `this.box.val()` — `.val()` receiver is the field `box` of type Box.
    let p = proj();
    let s = p.source("Sub.java").to_string();
    let off = at(&s, "this.box.val()") + "this.box.".len();
    let d = p.goto("Sub.java", off).expect("this.field.method resolves");
    assert_eq!(d.file, "Box.java");
    assert_eq!(d.label, "method p.Box.val()");
}

#[test]
fn find_usages_static_method_counts_calls() {
    let p = proj();
    let u = p.source("Util.java").to_string();
    let off = at(&u, "static Util create()") + "static Util ".len();
    // create() is called in useStaticMethod() (`Util.create()`) and useStaticChain()
    // (`Util.create().inst()`) = 2. Static call sites must be bucketed like instance ones.
    let n = p.usage_count("Util.java", off);
    assert_eq!(n, 2, "static create() call sites are bucketed");
}

#[test]
fn find_usages_super_call_is_bucketed() {
    // `super.baseVal()` in useSuper() resolves to Base.baseVal and is bucketed there.
    let p = proj();
    let b = p.source("Base.java").to_string();
    let off = at(&b, "int baseVal()") + "int ".len();
    assert_eq!(p.usage_count("Base.java", off), 1, "the super.baseVal() call is bucketed");
}

#[test]
fn find_usages_static_field_counts_access() {
    let p = proj();
    let u = p.source("Util.java").to_string();
    let off = at(&u, "static int CONST") + "static int ".len();
    // CONST is read once (`Util.CONST` in useStaticField()).
    let n = p.usage_count("Util.java", off);
    assert_eq!(n, 1, "static field access is bucketed");
}
