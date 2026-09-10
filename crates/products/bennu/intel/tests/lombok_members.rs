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
    for expected in [
        "getId",
        "setId",
        "getCustomer",
        "setCustomer",
        "isShipped",
        "setShipped",
    ] {
        assert!(
            labels.contains(&expected.to_string()),
            "expected {expected:?} in {labels:?}"
        );
    }
    // The fields themselves are `private`, and the receiver is in another class — so they are
    // hidden here for the same reason any private is. The accessors above are the whole point of
    // `@Data`: they are what this class CAN reach.
    assert!(
        !labels.contains(&"id".to_string()),
        "a private field stays private, got {labels:?}"
    );
}

#[test]
fn boolean_field_uses_is_getter_not_get() {
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "o.\n") + "o.".len();
    let labels = p.complete_labels("Use.java", off);
    assert!(
        labels.contains(&"isShipped".to_string()),
        "primitive boolean → isX, got {labels:?}"
    );
    assert!(
        !labels.contains(&"getShipped".to_string()),
        "no getX for a boolean, got {labels:?}"
    );
}

#[test]
fn hover_on_generated_getter_reports_owner() {
    let p = data_project();
    let s = p.source("Use.java").to_string();
    let off = at(&s, "a.getId()") + "a.".len();
    let h = p
        .hover("Use.java", off)
        .expect("hover resolves a Lombok getter");
    assert_eq!(h.kind, "method");
    assert_eq!(h.container.as_deref(), Some("shop.Order"));
    assert!(
        h.signature.contains("getId"),
        "signature names the getter, got {:?}",
        h.signature
    );
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
    let d = p
        .goto("Use.java", off)
        .expect("generated getter redirects to its field");
    assert_eq!(d.file, "Order.java");
    assert_eq!(
        d.label, "field shop.Order.id",
        "landed on the backing field"
    );
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
    let d = p
        .goto("Use.java", off)
        .expect("generated setter redirects to its field");
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
    assert!(
        labels.contains(&"is_attivo".to_string()),
        "getter keeps the field's name, got {labels:?}"
    );
    assert!(
        !labels.contains(&"isIs_attivo".to_string()),
        "no doubled `is`, got {labels:?}"
    );

    // Go-to on the generated getter redirects to the field it wraps — which here is named identically.
    let call = at(&src, "s.is_attivo()") + "s.".len();
    let d = p
        .goto("Use.java", call)
        .expect("the generated getter redirects to its field");
    assert_eq!(d.file, "StatoElenco.java");
    assert_eq!(d.label, "field shop.StatoElenco.is_attivo");
}

/// `@Accessors(chain = true, fluent = true)` names both accessors after the field, so the getter
/// `name()` and the setter `name(String)` differ ONLY in arity. Completion deduplicated by name+kind,
/// so the getter (offered first) swallowed the setter and the fluent setters looked unsupported.
#[test]
fn fluent_accessors_reach_completion_as_one_row_that_counts_both() {
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
    // Completion folds a name's overloads into ONE row (`collapse_overloads`) — accepting a
    // completion writes the name, not the arguments, so a second row would be a second chance to
    // choose with one outcome. What this test is really about is that BOTH accessors reach the fold:
    // Lombok generates them from one field, and a dedup keyed on the name alone used to drop one.
    assert_eq!(accessors.len(), 1, "one folded row, got {accessors:?}");
    assert!(
        accessors[0].contains("overload"),
        "the row must say the setter is there too, got {accessors:?}"
    );
    assert!(
        accessors.iter().any(|d| d.contains("() : String")),
        "the fluent getter returns it, got {accessors:?}"
    );
    // The chained setter's RETURN type (`Order`, from `chain = true`) is not asserted HERE: the
    // fold shows one detail and it is the getter's. It is observable on hover, which picks the
    // overload by the call's arity — see `the_chained_setter_is_what_hover_answers_for_a_one_arg_call`.
    // No get/set-prefixed names exist at all under `fluent`.
    let labels = p.complete_labels("Use.java", off);
    assert!(
        !labels
            .iter()
            .any(|l| l.starts_with("get") || l.starts_with("set")),
        "fluent accessors have no prefix, got {labels:?}"
    );
}

/// Hover picks the overload the CALL binds to, by arity.
///
/// The two fluent accessors differ only in arity — `customer()` returns `String`, `customer(String)`
/// returns `Order` because of `chain = true`. Hover used to take the first member of the name it met
/// walking the hierarchy, so pointing at `o.customer("x")` described the getter: a card stating the
/// wrong return type for the expression right under the caret.
#[test]
fn the_chained_setter_is_what_hover_answers_for_a_one_arg_call() {
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
             \x20   void run(Order o) { o.customer(\"x\"); String c = o.customer(); }\n\
             }\n",
        ),
    ]);
    let src = p.source("Use.java").to_string();

    let setter = at(&src, "o.customer(\"x\")") + "o.".len();
    let h = p.hover("Use.java", setter).expect("hover on the setter call");
    assert!(
        h.signature.contains("String") && h.signature.contains('('),
        "the one-argument overload was not chosen: {:?}",
        h.signature
    );
    assert!(
        !h.signature.contains("customer()"),
        "hover answered with the no-argument getter: {:?}",
        h.signature
    );

    let getter = at(&src, "o.customer()") + "o.".len();
    let h = p.hover("Use.java", getter).expect("hover on the getter call");
    assert!(
        h.signature.contains("customer()"),
        "the no-argument overload was not chosen: {:?}",
        h.signature
    );
}

/// The general case of the same thing: every overload of a name reaches completion and is folded
/// into one row carrying the count. What the dedup must NOT do is lose one on the way — a row
/// saying `+2 overloads` is the evidence all three arrived.
#[test]
fn overloads_are_folded_into_one_row_that_counts_them() {
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
    assert_eq!(renders.len(), 1, "one folded row, got {renders:?}");
    assert!(
        renders[0].contains("+2 overloads"),
        "all three arrived and the row says so, got {renders:?}"
    );
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
    let n = p
        .complete_labels("UseSub.java", off)
        .iter()
        .filter(|l| *l == "describe")
        .count();
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
    assert!(
        labels.contains(&"getX".to_string()),
        "@Value has getters, got {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l.starts_with("set")),
        "@Value is immutable, got {labels:?}"
    );
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
             \x20   public void go() {\n\
             \x20       this.\n\
             \x20   }\n\
             \x20   public void after() { }\n\
             }\n",
        ),
        (
            "UseSvc.java",
            "package s;\n\
             public class UseSvc {\n\
             \x20   void run(Svc svc) { svc.log.info(\"x\"); }\n\
             }\n",
        ),
    ]);
    // From INSIDE the class, which is the only place `log` is reachable: Lombok generates it
    // `private static final`, and the visibility rule hides it from an external receiver exactly
    // as it hides any other private field.
    let s = p.source("Svc.java").to_string();
    let off = at(&s, "this.\n") + "this.".len();
    let labels = p.complete_labels("Svc.java", off);
    assert!(
        labels.contains(&"log".to_string()),
        "@Slf4j injects a `log` field, got {labels:?}"
    );
    let outside = p.source("UseSvc.java").to_string();
    let from_outside = p.complete_labels("UseSvc.java", at(&outside, "svc.log") + "svc.".len());
    assert!(
        !from_outside.contains(&"log".to_string()),
        "…and it is private: {from_outside:?}"
    );
}

/// **`@Builder(toBuilder = true)` generates an instance `toBuilder()`**, the method the whole
/// pattern exists for: take this value, get a builder pre-filled from it, change one field, build.
/// Only the static `builder()` was modelled, so every `x.toBuilder()` in a codebase written this way
/// read as a method nobody declared.
///
/// The class here is the shape that reported it — `@Value @Builder(toBuilder = true) @Jacksonized`,
/// with a `@Builder.Default` field among plain ones.
#[test]
fn to_builder_is_generated_and_chains_like_the_static_factory() {
    let p = Project::new(&[
        (
            "AttributeEntry.java",
            "package shop;\n\
             import lombok.Builder;\n\
             import lombok.Value;\n\
             import lombok.extern.jackson.Jacksonized;\n\
             @Value\n\
             @Builder(toBuilder = true)\n\
             @Jacksonized\n\
             public class AttributeEntry {\n\
             \x20   String contributedBy;\n\
             \x20   @Builder.Default\n\
             \x20   String enforcement = \"MANDATORY\";\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   AttributeEntry with(AttributeEntry e) {\n\
             \x20       return e.toBuilder().contributedBy(\"me\").build();\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    assert_eq!(p.validate_errors("Use.java"), Vec::<String>::new());

    // And the chain is TYPED, not merely tolerated: hover on the call says what it returns.
    let s = p.source("Use.java").to_string();
    let h = p
        .hover("Use.java", at(&s, "toBuilder()") + 1)
        .expect("hover on the generated toBuilder");
    assert!(
        h.signature.contains("toBuilder"),
        "the card describes it, got {:?}",
        h.signature
    );
}

/// The `@Builder.Default` field is a constructor parameter, because Lombok moves its initializer
/// out of the field and into a `$default$…` method — leaving a blank final the constructor assigns.
/// Counting it out left the all-args constructor one parameter short, and calling the real one read
/// as the wrong arity.
#[test]
fn a_builder_default_field_is_still_a_constructor_parameter() {
    let p = Project::new(&[
        (
            "Entry.java",
            // The field is written `final` on purpose: that is the shape the exclusion was for — a
            // final field with an initializer cannot be assigned twice, so an all-args constructor
            // normally skips it. `@Builder.Default` is exactly the case where that reasoning is
            // wrong, and where the parameter comes back.
            "package shop;\n\
             import lombok.Builder;\n\
             @Builder\n\
             public class Entry {\n\
             \x20   private final String name;\n\
             \x20   @Builder.Default\n\
             \x20   private final String kind = \"SCALAR\";\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   Entry make() { return new Entry(\"a\", \"b\"); }\n\
             }\n",
        ),
    ]);
    assert_eq!(p.validate_errors("Use.java"), Vec::<String>::new());
}

/// `@Singular` is written to get the single-element adder — `.tag("a").tag("b")` — and the clearing
/// method beside it. Only the plural setter existed, so the idiomatic call was reported as missing.
#[test]
fn singular_adds_the_element_and_clear_methods_to_the_builder() {
    let p = Project::new(&[
        (
            "Post.java",
            "package shop;\n\
             import java.util.List;\n\
             import lombok.Builder;\n\
             import lombok.Singular;\n\
             @Builder\n\
             public class Post {\n\
             \x20   @Singular private List<String> tags;\n\
             \x20   @Singular(\"entry\") private List<String> entries;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   Post make() {\n\
             \x20       return Post.builder().tag(\"a\").tag(\"b\").clearTags().entry(\"x\").build();\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    assert_eq!(p.validate_errors("Use.java"), Vec::<String>::new());
}

/// `@SuperBuilder` builds a HIERARCHY: the subclass's builder carries the superclass's fields too,
/// and those live in another file — nothing available while the index is built can enumerate them.
/// A half-modelled builder is worse than none, because the missing half resolves against a type we
/// do model and reads as a method nobody declared. So the chain stays untyped and quiet.
#[test]
fn a_super_builder_over_a_parent_does_not_report_the_parents_own_setters() {
    let p = Project::new(&[
        (
            "Base.java",
            "package shop;\n\
             import lombok.experimental.SuperBuilder;\n\
             @SuperBuilder\n\
             public class Base {\n\
             \x20   private String id;\n\
             }\n",
        ),
        (
            "Child.java",
            "package shop;\n\
             import lombok.experimental.SuperBuilder;\n\
             @SuperBuilder\n\
             public class Child extends Base {\n\
             \x20   private String extra;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   Child make() { return Child.builder().id(\"1\").extra(\"x\").build(); }\n\
             }\n",
        ),
    ]);
    assert_eq!(p.validate_errors("Use.java"), Vec::<String>::new());
}

/// And a plain `@Builder` inherits nothing, so its builder stays fully modelled — a name that is
/// NOT a field of it is still reported. The silence above is the narrow case, not a blanket.
#[test]
fn a_plain_builder_still_reports_a_setter_that_does_not_exist() {
    let p = Project::new(&[
        (
            "Post.java",
            "package shop;\n\
             import lombok.Builder;\n\
             @Builder\n\
             public class Post {\n\
             \x20   private String title;\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   Post make() { return Post.builder().nope(\"x\").build(); }\n\
             }\n",
        ),
    ]);
    let errors = p.validate_errors("Use.java");
    assert!(
        errors.iter().any(|e| e.contains("nope")),
        "a setter that is no field must still be reported, got {errors:?}"
    );
}

/// **`@Value` makes the fields `private final` and the class `final`.**
///
/// The source writes them bare — that is the point of the annotation — so read as written they were
/// package-private and mutable to everything downstream, and the class looked extendable when javac
/// says it is not. `@NonFinal` on a field is the documented way out and is honoured.
#[test]
fn value_makes_its_fields_final_and_its_class_final() {
    let p = Project::new(&[(
        "Money.java",
        "package shop;\n\
         import lombok.Value;\n\
         import lombok.experimental.NonFinal;\n\
         @Value\n\
         public class Money {\n\
         \x20   String currency;\n\
         \x20   @NonFinal String note;\n\
         }\n",
    )]);
    let cm = p.members("shop/Money").expect("the value class is indexed");
    assert!(cm.flags.is_final, "a @Value class is final");
    let field = |n: &str| cm.fields.iter().find(|f| f.name == n).expect("field");
    assert!(field("currency").is_final, "@Value makes a field final");
    assert_eq!(
        field("currency").visibility,
        bennu_java::prelude::Visibility::Private,
        "and private"
    );
    assert!(!field("note").is_final, "@NonFinal opts the field out");
}

/// `@Builder` on a **static factory** builds what the factory returns, from the factory's own
/// parameters — not from the class's fields. The builder is named after the return type, which is
/// Lombok's rule and the reason a class can carry more than one.
#[test]
fn a_builder_on_a_static_method_takes_its_parameters() {
    let p = Project::new(&[
        (
            "Report.java",
            "package shop;\n\
             import lombok.Builder;\n\
             public class Report {\n\
             \x20   private String unrelatedField;\n\
             \x20   @Builder\n\
             \x20   public static Report of(String title, int pages) { return null; }\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   Report make() { return Report.builder().title(\"t\").pages(3).build(); }\n\
             }\n",
        ),
    ]);
    assert_eq!(p.validate_errors("Use.java"), Vec::<String>::new());
    // And it is the FACTORY's parameters, not the class's fields: a field of the class is not a
    // setter on this builder.
    let cm = p
        .members("shop/Report/ReportBuilder")
        .expect("the builder type is indexed");
    let names: Vec<&str> = cm.methods.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"title") && names.contains(&"pages"), "got {names:?}");
    assert!(!names.contains(&"unrelatedField"), "got {names:?}");
}

/// `@Delegate` copies every public method of the field's type onto the owner. Which methods those
/// are is a question about another type, and there is no answer while this index is built — so the
/// type says its list is incomplete, and no check concludes "no such method" from it.
#[test]
fn a_delegate_field_stops_the_owner_being_reported_for_missing_methods() {
    let p = Project::new(&[
        (
            "Bag.java",
            "package shop;\n\
             import java.util.ArrayList;\n\
             import lombok.experimental.Delegate;\n\
             public class Bag {\n\
             \x20   @Delegate private final ArrayList<String> items = new ArrayList<>();\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   void run(Bag b) { b.add(\"x\"); b.clear(); }\n\
             }\n",
        ),
    ]);
    assert_eq!(p.validate_errors("Use.java"), Vec::<String>::new());
}

/// And the silence is exactly that class: one WITHOUT a `@Delegate` still reports a method nobody
/// declares.
#[test]
fn a_class_without_a_delegate_still_reports_a_method_that_does_not_exist() {
    let p = Project::new(&[
        (
            "Plain.java",
            "package shop;\n\
             public class Plain {\n\
             \x20   public void known() {}\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   void run(Plain p) { p.nope(); }\n\
             }\n",
        ),
    ]);
    let errors = p.validate_errors("Use.java");
    assert!(errors.iter().any(|e| e.contains("nope")), "got {errors:?}");
}
