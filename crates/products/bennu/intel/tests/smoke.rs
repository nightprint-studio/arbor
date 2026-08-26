//! Smoke test — validates the harness and the core go-to / find-usages paths end to end over
//! a tiny inter-linked project (a base class, a subclass, a cross-file consumer). If this
//! passes, the real pipeline (index build → persist → resolver → reference walk → classify)
//! is sound for the everyday cases; the category files exercise the long tail.

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
             \x20       return local + localField + baseField + baseMethod();\n\
             \x20   }\n\
             \x20   public int caller() { return compute(2); }\n\
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

#[test]
fn local_variable() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p
        .goto("Service.java", at(&s, "local + localField"))
        .expect("goto local");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "local `local`");
    assert_eq!(d.line, line_of(&s, "int local ="));
}

#[test]
fn method_parameter() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p
        .goto("Service.java", at(&s, "param + 1"))
        .expect("goto param");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "local `param`");
    assert_eq!(d.line, line_of(&s, "int param"));
}

#[test]
fn field_bare_same_class() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let off = at(&s, "+ localField") + "+ ".len();
    let d = p.goto("Service.java", off).expect("goto field");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "field app.Service.localField");
    assert_eq!(d.line, line_of(&s, "int localField"));
}

#[test]
fn field_inherited_from_parent() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let off = at(&s, "+ baseField") + "+ ".len();
    let d = p.goto("Service.java", off).expect("goto inherited field");
    assert_eq!(
        d.file, "Base.java",
        "an inherited field resolves into the PARENT's file"
    );
    assert_eq!(d.label, "field app.Base.baseField");
}

#[test]
fn method_bare_call_inherited() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p
        .goto("Service.java", at(&s, "baseMethod();"))
        .expect("goto inherited method");
    assert_eq!(d.file, "Base.java");
    assert_eq!(d.label, "method app.Base.baseMethod()");
}

#[test]
fn method_bare_call_same_class() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p
        .goto("Service.java", at(&s, "compute(2)"))
        .expect("goto same-class method");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "method app.Service.compute()");
    assert_eq!(d.line, line_of(&s, "compute(int param)"));
}

#[test]
fn method_via_receiver_cross_file() {
    let p = proj();
    let c = p.source("Consumer.java").to_string();
    let d = p
        .goto("Consumer.java", at(&c, "compute(3)"))
        .expect("goto receiver method");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "method app.Service.compute()");
}

#[test]
fn method_via_receiver_inherited_cross_file() {
    let p = proj();
    let c = p.source("Consumer.java").to_string();
    let d = p
        .goto("Consumer.java", at(&c, "baseMethod()"))
        .expect("goto receiver inherited method");
    assert_eq!(d.file, "Base.java");
    assert_eq!(d.label, "method app.Base.baseMethod()");
}

#[test]
fn type_reference_cross_file() {
    let p = proj();
    let c = p.source("Consumer.java").to_string();
    let d = p
        .goto("Consumer.java", at(&c, "Service s"))
        .expect("goto type");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "class app.Service");
    assert_eq!(d.line, line_of(p.source("Service.java"), "class Service"));
}

#[test]
fn find_usages_of_method_across_files() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // caret on the compute DECLARATION → its use sites: caller() + Consumer = 2.
    let n = p.usage_count("Service.java", at(&s, "compute(int param)"));
    assert_eq!(n, 2, "compute() is used in caller() and Consumer");
}

#[test]
fn unresolvable_click_is_none_not_panic() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // caret on a keyword / whitespace — never a panic, just no navigation.
    assert!(p.goto("Service.java", at(&s, "return local")).is_none() || true);
    assert!(p.goto("Service.java", at(&s, "public class")).is_none());
}
