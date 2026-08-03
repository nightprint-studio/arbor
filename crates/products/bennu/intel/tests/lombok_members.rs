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

/// A `@Getter` enum whose primitive `boolean` field is named `is_attivo`: Lombok strips the field's
/// own `is` (what follows it is not a lowercase letter), so the getter is `is_attivo()` — the field's
/// exact name. Bennu named it `isIs_attivo`, so a real getter read as unresolvable everywhere.
/// Go-to on it lands on the field, whose name the accessor happens to share.
#[test]
fn boolean_is_underscore_getter_resolves_on_an_enum() {
    let p = Project::new(&[
        (
            "StatoElenco.java",
            "package shop;\n\
             import lombok.Getter;\n\
             import lombok.RequiredArgsConstructor;\n\
             @Getter\n\
             @RequiredArgsConstructor\n\
             public enum StatoElenco {\n\
             \x20   ATTIVO(true),\n\
             \x20   ARCHIVIATO(false);\n\
             \x20   private final boolean is_attivo;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   boolean run(StatoElenco s) { return s.is_attivo(); }\n\
             \x20   void offer(StatoElenco s) { s.\n }\n\
             }\n",
        ),
    ]);
    let src = p.source("Use.java").to_string();
    let off = at(&src, "s.\n") + "s.".len();
    let labels = p.complete_labels("Use.java", off);
    assert!(labels.contains(&"is_attivo".to_string()), "getter keeps the field's name, got {labels:?}");
    assert!(!labels.contains(&"isIs_attivo".to_string()), "no doubled `is`, got {labels:?}");

    // Go-to on the generated getter redirects to the field it wraps — which here is named identically.
    let call = at(&src, "s.is_attivo()") + "s.".len();
    let d = p.goto("Use.java", call).expect("the generated getter redirects to its field");
    assert_eq!(d.file, "StatoElenco.java");
    assert_eq!(d.label, "field shop.StatoElenco.is_attivo");
}

/// `@Accessors(chain = true, fluent = true)` names both accessors after the field, so the getter
/// `name()` and the setter `name(String)` differ ONLY in arity. Completion deduplicated by name+kind,
/// so the getter (offered first) swallowed the setter and the fluent setters looked unsupported.
#[test]
fn fluent_accessors_offer_both_the_getter_and_the_setter() {
    let p = Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             import lombok.Data;\n\
             import lombok.experimental.Accessors;\n\
             @Data\n\
             @Accessors(chain = true, fluent = true)\n\
             public class Order {\n\
             \x20   private String customer;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   void run(Order o) { o.\n }\n\
             }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let off = at(&s, "o.\n") + "o.".len();
    let items = p.complete("Use.java", off);
    let accessors: Vec<&str> = items
        .iter()
        .filter(|i| i.label == "customer" && i.kind == "method")
        .filter_map(|i| i.detail.as_deref())
        .collect();
    assert_eq!(accessors.len(), 2, "getter AND setter, got {accessors:?}");
    assert!(
        accessors.iter().any(|d| d.contains("customer(String)")),
        "the fluent setter takes the field's type, got {accessors:?}"
    );
    assert!(
        accessors.iter().any(|d| d.contains("customer() : String")),
        "the fluent getter returns it, got {accessors:?}"
    );
    // `chain = true` → the setter returns the owner, so `o.customer("x").customer("y")` chains.
    assert!(
        accessors.iter().any(|d| d.contains("customer(String) : Order")),
        "chained setter returns the owner, got {accessors:?}"
    );
    // No get/set-prefixed names exist at all under `fluent`.
    let labels = p.complete_labels("Use.java", off);
    assert!(
        !labels.iter().any(|l| l.starts_with("get") || l.starts_with("set")),
        "fluent accessors have no prefix, got {labels:?}"
    );
}

/// The same dedup collapsed every ordinary **overload**, Lombok or not — this is the general case.
#[test]
fn overloads_are_offered_one_entry_per_signature() {
    let p = Project::new(&[
        (
            "Fmt.java",
            "package util;\n\
             public class Fmt {\n\
             \x20   public String render(String s) { return s; }\n\
             \x20   public String render(String s, int width) { return s; }\n\
             \x20   public String render(int n) { return \"\"; }\n\
             }\n",
        ),
        (
            "UseFmt.java",
            "package util;\n\
             public class UseFmt {\n\
             \x20   void run(Fmt f) { f.\n }\n\
             }\n",
        ),
    ]);
    let s = p.source("UseFmt.java").to_string();
    let off = at(&s, "f.\n") + "f.".len();
    let renders: Vec<String> = p
        .complete("UseFmt.java", off)
        .into_iter()
        .filter(|i| i.label == "render")
        .filter_map(|i| i.detail)
        .collect();
    assert_eq!(renders.len(), 3, "all three overloads, got {renders:?}");
}

/// The dedup still has to do its actual job: an override must not appear twice, once from the
/// subclass and once from the supertype that declares the same signature.
#[test]
fn an_override_is_still_offered_once() {
    let p = Project::new(&[
        (
            "Base.java",
            "package h;\n\
             public class Base {\n\
             \x20   public String describe() { return \"base\"; }\n\
             }\n",
        ),
        (
            "Sub.java",
            "package h;\n\
             public class Sub extends Base {\n\
             \x20   @Override public String describe() { return \"sub\"; }\n\
             }\n",
        ),
        (
            "UseSub.java",
            "package h;\n\
             public class UseSub {\n\
             \x20   void run(Sub s) { s.\n }\n\
             }\n",
        ),
    ]);
    let src = p.source("UseSub.java").to_string();
    let off = at(&src, "s.\n") + "s.".len();
    let n = p.complete_labels("UseSub.java", off).iter().filter(|l| *l == "describe").count();
    assert_eq!(n, 1, "the override collapses with the method it overrides");
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
