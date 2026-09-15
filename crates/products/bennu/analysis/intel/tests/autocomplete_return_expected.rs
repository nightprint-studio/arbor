//! Autocompletion category — **what a `return` wants**, first.
//!
//! The reported case: in a method returning `Order`, `return builder.|` did not open on `build()`.
//! The expected type was known and was one weighted term among several, and a builder whose setters
//! the method had already written (and the session had already picked) outscored the one member
//! that compiles there. These tests pin the expected type as the LEADING key — after a dot, for a
//! bare word, inside a lambda, through generics and primitives — and that nothing is hidden by it.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

fn rank_of(labels: &[String], name: &str) -> usize {
    labels.iter().position(|l| l == name).unwrap_or(usize::MAX)
}

fn after(p: &Project, file: &str, needle: &str) -> usize {
    at(p.source(file), needle) + needle.len()
}

fn labels(items: &[CompletionItem]) -> Vec<String> {
    items.iter().map(|i| i.label.clone()).collect()
}

// ── A Lombok `@Builder` ──────────────────────────────────────────────────────────────────────

/// The user's own shape: an explicitly typed `Order.OrderBuilder`, its setters written above.
fn lombok_builder(trigger: &str) -> Project {
    Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             import lombok.Builder;\n\
             @Builder\n\
             public class Order {\n\
             \x20   private long id;\n\
             \x20   private String customer;\n\
             }\n",
        ),
        (
            "OrderService.java",
            &format!(
                "package shop;\n\
                 public class OrderService {{\n\
                 \x20   public Order create(long id, String customer) {{\n\
                 \x20       Order.OrderBuilder builder = Order.builder();\n\
                 \x20       builder.customer(customer);\n\
                 \x20       builder.id(id);\n\
                 \x20       builder.customer(customer);\n\
                 \x20       {trigger}\n\
                 \x20   }}\n\
                 }}\n"
            ),
        ),
    ])
}

#[test]
fn return_after_a_lombok_builder_opens_on_build() {
    let p = lombok_builder("return builder.");
    let items = p.complete("OrderService.java", after(&p, "OrderService.java", "return builder."));
    let first = items.first().expect("the builder's members are offered");
    assert_eq!(first.label, "build", "{:?}", labels(&items));
    assert!(first.preselect, "the only member returning `Order` is the answer");
    // Ranked, not filtered: the setters are still there.
    assert!(labels(&items).contains(&"customer".to_string()), "{:?}", labels(&items));
}

#[test]
fn a_typed_prefix_keeps_build_first() {
    let p = lombok_builder("return builder.b");
    let items = p.complete("OrderService.java", after(&p, "OrderService.java", "return builder.b"));
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("build"), "{:?}", labels(&items));
}

// ── A hand-written builder ───────────────────────────────────────────────────────────────────

fn plain_builder(trigger: &str) -> Project {
    Project::new(&[
        (
            "Invoice.java",
            "package shop;\n\
             public class Invoice {\n\
             \x20   public static InvoiceBuilder builder() { return new InvoiceBuilder(); }\n\
             }\n",
        ),
        (
            "InvoiceBuilder.java",
            "package shop;\n\
             public class InvoiceBuilder {\n\
             \x20   public InvoiceBuilder customer(String c) { return this; }\n\
             \x20   public InvoiceBuilder amount(int a) { return this; }\n\
             \x20   public String describe() { return \"\"; }\n\
             \x20   public void reset() { }\n\
             \x20   public Invoice build() { return new Invoice(); }\n\
             }\n",
        ),
        (
            "Billing.java",
            &format!(
                "package shop;\n\
                 public class Billing {{\n\
                 \x20   public Invoice create(String customer) {{\n\
                 \x20       InvoiceBuilder builder = Invoice.builder();\n\
                 \x20       builder.customer(customer);\n\
                 \x20       builder.customer(customer);\n\
                 \x20       builder.amount(3);\n\
                 \x20       {trigger}\n\
                 \x20   }}\n\
                 }}\n"
            ),
        ),
    ])
}

#[test]
fn return_after_a_hand_written_builder_opens_on_build() {
    let p = plain_builder("return builder.");
    let items = p.complete("Billing.java", after(&p, "Billing.java", "return builder."));
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("build"), "{:?}", labels(&items));
    assert!(items[0].preselect);
}

/// The root cause, pinned: a habit — the setter picked four times this session, and written three
/// times in the method — must not outrank the one member that compiles after the `return`.
#[test]
fn a_frequently_picked_setter_does_not_outrank_what_the_return_wants() {
    for _ in 0..4 {
        bennu_query::prelude::record_pick("shop/InvoiceBuilder", "customer");
    }
    let p = plain_builder("return builder.");
    let got = labels(&p.complete("Billing.java", after(&p, "Billing.java", "return builder.")));
    assert_eq!(got.first().map(String::as_str), Some("build"), "{got:?}");
}

// ── Primitives and generics ──────────────────────────────────────────────────────────────────

#[test]
fn a_primitive_return_opens_on_the_member_producing_it() {
    let p = Project::new(&[
        (
            "Basket.java",
            "package shop;\n\
             public class Basket {\n\
             \x20   public String label() { return \"\"; }\n\
             \x20   public boolean empty() { return true; }\n\
             \x20   public Basket copy() { return this; }\n\
             \x20   public int size() { return 0; }\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   public int count(Basket basket) {\n\
             \x20       return basket.\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    let items = p.complete("Use.java", after(&p, "Use.java", "return basket."));
    assert_eq!(items.first().map(|i| i.label.as_str()), Some("size"), "{:?}", labels(&items));
}

/// `Bag<Order>` is returned: `orders()` exactly, then `listed()` — a `ListBag<Order>`, a subtype —
/// then the rest, `names()` among them, since a `Bag<String>` is not a `Bag<Order>`.
#[test]
fn a_generic_return_puts_the_exact_type_then_a_subtype_first() {
    let p = Project::new(&[
        ("Order.java", "package shop;\npublic class Order { }\n"),
        ("Bag.java", "package shop;\npublic interface Bag<T> { }\n"),
        ("ListBag.java", "package shop;\npublic class ListBag<T> implements Bag<T> { }\n"),
        (
            "Shelf.java",
            "package shop;\n\
             public class Shelf {\n\
             \x20   public String label() { return \"\"; }\n\
             \x20   public Bag<String> names() { return null; }\n\
             \x20   public ListBag<Order> listed() { return null; }\n\
             \x20   public Bag<Order> orders() { return null; }\n\
             }\n",
        ),
        (
            "Use.java",
            "package shop;\n\
             public class Use {\n\
             \x20   public Bag<Order> pick(Shelf shelf) {\n\
             \x20       return shelf.\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    let got = p.complete_labels("Use.java", after(&p, "Use.java", "return shelf."));
    assert_eq!(got.first().map(String::as_str), Some("orders"), "{got:?}");
    assert!(rank_of(&got, "listed") < rank_of(&got, "label"), "{got:?}");
    assert!(rank_of(&got, "listed") < rank_of(&got, "names"), "{got:?}");
}

// ── Inside a lambda ──────────────────────────────────────────────────────────────────────────

fn kitchen(body: &str) -> Project {
    Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             public class Order {\n\
             \x20   public String title() { return \"\"; }\n\
             \x20   public int count() { return 0; }\n\
             \x20   public Order copy() { return this; }\n\
             }\n",
        ),
        ("Maker.java", "package shop;\npublic interface Maker {\n    Order make();\n}\n"),
        (
            "Kitchen.java",
            &format!("package shop;\npublic class Kitchen {{\n    void accept(Maker m) {{ }}\n{body}\n}}\n"),
        ),
    ])
}

/// The lambda returns to `Maker.make` — `Order` — and not to the `String` method it sits in.
#[test]
fn a_return_in_a_lambda_assigned_to_a_functional_interface_wants_its_return() {
    let p = kitchen(
        "    public String serve(Order o) {\n\
         \x20       Maker m = () -> {\n\
         \x20           return o.\n\
         \x20       };\n\
         \x20       return \"\";\n\
         \x20   }",
    );
    let got = p.complete_labels("Kitchen.java", after(&p, "Kitchen.java", "return o."));
    assert_eq!(got.first().map(String::as_str), Some("copy"), "{got:?}");
}

#[test]
fn a_return_in_a_lambda_passed_as_an_argument_wants_its_return() {
    let p = kitchen(
        "    public String serve(Order o) {\n\
         \x20       accept(() -> {\n\
         \x20           return o.\n\
         \x20       });\n\
         \x20       return \"\";\n\
         \x20   }",
    );
    let got = p.complete_labels("Kitchen.java", after(&p, "Kitchen.java", "return o."));
    assert_eq!(got.first().map(String::as_str), Some("copy"), "{got:?}");
}

// ── A bare word ──────────────────────────────────────────────────────────────────────────────

fn picker(trigger: &str) -> Project {
    Project::new(&[
        ("Order.java", "package shop;\npublic class Order { }\n"),
        (
            "Picker.java",
            &format!(
                "package shop;\n\
                 public class Picker {{\n\
                 \x20   private Order current;\n\
                 \x20   private String title;\n\
                 \x20   public Order pick(String name) {{\n\
                 \x20       Order order = current;\n\
                 \x20       String label = name;\n\
                 \x20       {trigger}\n\
                 \x20   }}\n\
                 }}\n"
            ),
        ),
    ])
}

/// `return |`: the local and the FIELD of type `Order` above the nearer `String` local — the fit
/// crosses the nearness bands, which is the only thing that can put a field above a local.
#[test]
fn a_bare_return_puts_the_names_of_the_returned_type_first() {
    let p = picker("return ");
    let got = p.scope_labels("Picker.java", after(&p, "Picker.java", "return "));
    assert!(rank_of(&got, "order") < rank_of(&got, "label"), "{got:?}");
    assert!(rank_of(&got, "current") < rank_of(&got, "label"), "{got:?}");
    assert!(rank_of(&got, "current") < rank_of(&got, "name"), "{got:?}");
    // Nothing is hidden.
    assert!(got.contains(&"label".to_string()) && got.contains(&"title".to_string()), "{got:?}");
}

/// Two names of the right type are a choice; one is an answer.
#[test]
fn the_only_bare_name_of_the_returned_type_is_preselected() {
    let p = picker("return or");
    let items = p.scope_complete("Picker.java", after(&p, "Picker.java", "return or"));
    let first = items.first().expect("`order` is offered");
    assert_eq!(first.label, "order", "{:?}", labels(&items));
    assert!(first.preselect);

    let p = picker("return ");
    let items = p.scope_complete("Picker.java", after(&p, "Picker.java", "return "));
    assert!(items.iter().all(|i| !i.preselect), "{:?}", labels(&items));
}
