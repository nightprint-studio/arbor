//! Lombok generated-member awareness (end-to-end over the real index).
//!
//! Lombok generates getters/setters/`log` at compile time; bennu synthesizes them at index-build
//! so they resolve through the same members path as real declarations. What WORKS on a generated
//! member: **completion**, **hover**, **find-usages** (all go through `members_of`). What does NOT:
//! **go-to** on a generated member returns `None` — there is no source name token to open (a
//! documented limitation; go-to on the underlying FIELD still works). These tests pin all four.

mod common;
use common::*;

fn data_project() -> Project {
    Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             import lombok.Data;\n\
             @Data\n\
             public class Order {\n\
             \x20   private long id;\n\
             \x20   private String customer;\n\
             \x20   private boolean shipped;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   public void run(Order o) {\n\
             \x20       o.\n\
             \x20   }\n\
             \x20   public long twice(Order a, Order b) {\n\
             \x20       return a.getId() + b.getId();\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

#[test]
fn completion_offers_lombok_getters_and_setters() {
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "o.\n") + "o.".len();
    let labels = p.complete_labels("Use.java", off);
    for expected in ["getId", "setId", "getCustomer", "setCustomer", "isShipped", "setShipped"] {
        assert!(labels.contains(&expected.to_string()), "expected {expected:?} in {labels:?}");
    }
    // The real fields are still offered too.
    assert!(labels.contains(&"id".to_string()), "declared field still offered, got {labels:?}");
}

#[test]
fn boolean_field_uses_is_getter_not_get() {
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "o.\n") + "o.".len();
    let labels = p.complete_labels("Use.java", off);
    assert!(labels.contains(&"isShipped".to_string()), "primitive boolean → isX, got {labels:?}");
    assert!(!labels.contains(&"getShipped".to_string()), "no getX for a boolean, got {labels:?}");
}

#[test]
fn hover_on_generated_getter_reports_owner() {
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "a.getId()") + "a.".len();
    let h = p.hover("Use.java", off).expect("hover resolves a Lombok getter");
    assert_eq!(h.kind, "method");
    assert_eq!(h.container.as_deref(), Some("shop.Order"));
    assert!(h.signature.contains("getId"), "signature names the getter, got {:?}", h.signature);
}

#[test]
fn find_usages_of_generated_getter_counts_calls() {
    // `getId()` is called twice in twice(); find-usages buckets both (the walk resolves the call
    // receiver to Order and finds the synthetic member there, same as the query).
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "a.getId()") + "a.".len();
    let n = p.usage_count("Use.java", off);
    assert_eq!(n, 2, "both a.getId() and b.getId() are bucketed");
}

#[test]
fn goto_on_generated_getter_redirects_to_backing_field() {
    // A generated getter has no source name token — go-to redirects to the FIELD it wraps.
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "a.getId()") + "a.".len();
    let d = p.goto("Use.java", off).expect("generated getter redirects to its field");
    assert_eq!(d.file, "Order.java");
    assert_eq!(d.label, "field shop.Order.id", "landed on the backing field");
    assert_eq!(d.line, line_of(p.source("Order.java"), "long id;"));
}

#[test]
fn goto_on_generated_setter_redirects_to_backing_field() {
    let p = Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             import lombok.Data;\n\
             @Data\n\
             public class Order {\n\
             \x20   private String customer;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   void run(Order o) { o.setCustomer(\"x\"); }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "o.setCustomer(") + "o.".len();
    let d = p.goto("Use.java", off).expect("generated setter redirects to its field");
    assert_eq!(d.file, "Order.java");
    assert_eq!(d.label, "field shop.Order.customer");
}

#[test]
fn goto_on_the_backing_field_still_works() {
    let p = data_project();
    let s = p.source("Order.java").to_string();
    let off = at(&s, "long id;") + "long ".len();
    let d = p.goto("Order.java", off).expect("the real field resolves");
    assert_eq!(d.file, "Order.java");
    assert_eq!(d.label, "field shop.Order.id");
}

#[test]
fn value_annotation_is_getters_only() {
    let p = Project::new(&[
        (
            "Point.java",
            "package geo;\n\
             import lombok.Value;\n\
             @Value\n\
             public class Point {\n\
             \x20   int x;\n\
             \x20   int y;\n\
             }\n",
        ),
        (
            "UsePoint.java",
            "package geo;\n\
             public class UsePoint {\n\
             \x20   void run(Point p) { p.\n }\n\
             }\n",
        ),
    ]);
    let s = p.source("UsePoint.java").to_string();
    let off = at(&s, "p.\n") + "p.".len();
    let labels = p.complete_labels("UsePoint.java", off);
    assert!(labels.contains(&"getX".to_string()), "@Value has getters, got {labels:?}");
    assert!(!labels.iter().any(|l| l.starts_with("set")), "@Value is immutable, got {labels:?}");
}

#[test]
fn user_declared_getter_is_not_shadowed_by_synthetic() {
    // A hand-written getId() (that the user can navigate to) must not be duplicated by the synth,
    // and go-to on it must resolve to the REAL declaration.
    let p = Project::new(&[
        (
            "Acc.java",
            "package a;\n\
             import lombok.Data;\n\
             @Data\n\
             public class Acc {\n\
             \x20   private long id;\n\
             \x20   public long getId() { return id * 2; }\n\
             }\n",
        ),
        (
            "UseAcc.java",
            "package a;\n\
             public class UseAcc {\n\
             \x20   long run(Acc x) { return x.getId(); }\n\
             }\n",
        ),
    ]);
    // Go-to on x.getId() lands on the user's real getId() declaration in Acc.java.
    let u = p.source("UseAcc.java").to_string();
    let off = at(&u, "x.getId()") + "x.".len();
    let d = p.goto("UseAcc.java", off).expect("user getId() resolves");
    assert_eq!(d.file, "Acc.java");
    assert_eq!(d.label, "method a.Acc.getId()");
}

#[test]
fn slf4j_injects_a_log_field() {
    let p = Project::new(&[
        (
            "Svc.java",
            "package s;\n\
             import lombok.extern.slf4j.Slf4j;\n\
             @Slf4j\n\
             public class Svc {\n\
             \x20   public void go() { }\n\
             }\n",
        ),
        (
            "UseSvc.java",
            "package s;\n\
             public class UseSvc {\n\
             \x20   void run(Svc svc) { svc.\n }\n\
             }\n",
        ),
    ]);
    let s = p.source("UseSvc.java").to_string();
    let off = at(&s, "svc.\n") + "svc.".len();
    let labels = p.complete_labels("UseSvc.java", off);
    assert!(labels.contains(&"log".to_string()), "@Slf4j injects a `log` field, got {labels:?}");
}
