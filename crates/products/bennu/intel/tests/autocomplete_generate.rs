//! Autocompletion category — **members that do not exist yet**.
//!
//! Typing `getNa` in a class body should offer `getName()` and write it. It is not a guess: a
//! field with no getter is a fact about the text, the name is Java's own convention, and the body
//! is the only body it could have. These tests are about that — and, just as much, about the three
//! places it must stay quiet, because a completion that generates a method where you meant to call
//! one is worse than one that offers nothing.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

fn cls(body: &str) -> Project {
    Project::new(&[(
        "Order.java",
        &format!("package shop;\npublic class Order {{\n{body}\n}}\n"),
    )])
}

/// What would be GENERATED at the caret after `needle`. The list the popup merges into what
/// already exists — see `NativeJavaProvider::complete_at`.
fn generated_at(p: &Project, needle: &str) -> Vec<CompletionItem> {
    let s = p.source("Order.java").to_string();
    let off = at(&s, needle) + needle.len();
    p.generated("Order.java", off)
}

fn labels_at(p: &Project, needle: &str) -> Vec<String> {
    generated_at(p, needle).into_iter().map(|i| i.label).collect()
}

fn item_at(p: &Project, needle: &str, label: &str) -> CompletionItem {
    generated_at(p, needle)
        .into_iter()
        .find(|i| i.label == label)
        .unwrap_or_else(|| panic!("no candidate {label:?}"))
}

// ── What is offered ──────────────────────────────────────────────────────────────────────────

#[test]
fn a_field_with_no_getter_offers_one() {
    let p = cls("    private String customer;\n    getCu");
    assert!(labels_at(&p, "getCu").contains(&"getCustomer".to_string()));
}

#[test]
fn a_field_with_no_setter_offers_one() {
    let p = cls("    private String customer;\n    setCu");
    assert!(labels_at(&p, "setCu").contains(&"setCustomer".to_string()));
}

/// The camel humps reach a member that has not been written any more than one that has.
#[test]
fn the_humps_reach_a_generated_accessor() {
    let p = cls("    private String customerName;\n    gCN");
    assert!(labels_at(&p, "gCN").contains(&"getCustomerName".to_string()));
}

/// JavaBeans says `is` for a primitive `boolean`, and every framework that reflects over
/// accessors reads it that way.
#[test]
fn a_boolean_field_offers_an_is_getter() {
    let p = cls("    private boolean shipped;\n    isSh");
    assert!(labels_at(&p, "isSh").contains(&"isShipped".to_string()));
}

/// Accepting writes the whole method, not its name.
#[test]
fn accepting_writes_the_member() {
    let p = cls("    private String customer;\n    getCu");
    let it = item_at(&p, "getCu", "getCustomer");
    let text = it.insert_text.expect("the member is the insertion");
    assert!(text.starts_with("public String getCustomer() {"), "{text}");
    assert!(text.contains("return customer;"), "{text}");
    // Its own kind: this row does not name something that exists, and drawing it identically to
    // one that does would be claiming it did.
    assert_eq!(it.kind, "generate");
}

/// The generated lines are indented to where the caret is, not to column zero.
#[test]
fn the_member_is_written_at_the_carets_indentation() {
    let p = cls("    private String customer;\n    getCu");
    let text = item_at(&p, "getCu", "getCustomer").insert_text.unwrap();
    assert!(text.contains("\n        return customer;\n    }"), "{text:?}");
}

// ── Where it must stay quiet ─────────────────────────────────────────────────────────────────

/// The one Generate would skip too.
#[test]
fn a_field_that_already_has_a_getter_offers_nothing_to_generate() {
    let p = cls(
        "    private String customer;\n\
         \x20   public String getCustomer() { return customer; }\n\
         \x20   getCu",
    );
    assert!(generated_at(&p, "\n    getCu").is_empty(), "{:?}", labels_at(&p, "\n    getCu"));
}

/// Inside a method body `getCustomer` is a CALL. Generating a method there would put a member
/// declaration inside another one.
#[test]
fn nothing_is_generated_inside_a_method_body() {
    let p = cls(
        "    private String customer;\n\
         \x20   void go() {\n\
         \x20       getCu\n\
         \x20   }",
    );
    assert!(generated_at(&p, "getCu").is_empty());
}

/// `recv.getCu` is a member access — a call to something that may well exist elsewhere.
#[test]
fn nothing_is_generated_after_a_dot() {
    let p = cls(
        "    private String customer;\n\
         \x20   void go(Order o) {\n\
         \x20       o.getCu\n\
         \x20   }",
    );
    assert!(generated_at(&p, "o.getCu").is_empty());
}

/// A class body with three fields must not open on six generated members the moment the caret
/// lands in it, ahead of everything real.
#[test]
fn an_empty_prefix_generates_nothing() {
    let p = cls("    private String customer;\n    ");
    assert!(generated_at(&p, "customer;\n    ").is_empty());
}

/// A `final` field is assigned once, at construction.
#[test]
fn a_final_field_offers_no_setter() {
    let p = cls("    private final long id;\n    setI");
    assert!(!labels_at(&p, "setI").contains(&"setId".to_string()));
}

// ── Ghost text ───────────────────────────────────────────────────────────────────────────────
//
// The same candidates, drawn ahead of the caret instead of in a list. The bar is different and
// higher: ghost text sits inline, where it reads like text that is already there, so being wrong
// costs trust rather than a keystroke. Certainty, not likelihood.

fn hint_at(p: &Project, needle: &str) -> Option<bennu_intel::prelude::AccessorHint> {
    let s = p.source("Order.java").to_string();
    let off = at(&s, needle) + needle.len();
    p.hint("Order.java", off)
}

#[test]
fn one_candidate_is_drawn_ahead_of_the_caret() {
    let p = cls("    private String customer;\n    getCu");
    let hint = hint_at(&p, "getCu").expect("a hint");
    assert!(hint.insert.starts_with("public String getCustomer() {"), "{}", hint.insert);
}

/// Previewed as an arrow and the result. Accepting REPLACES what has been typed, and the member
/// drawn butting up against the `getCu` it is about to consume reads as `getCupublic String …`.
#[test]
fn the_preview_says_it_is_a_replacement() {
    let p = cls("    private String customer;\n    getCu");
    let hint = hint_at(&p, "getCu").expect("a hint");
    assert!(hint.preview.starts_with(" → "), "{:?}", hint.preview);
    assert!(hint.preview.contains("getCustomer"), "{:?}", hint.preview);
}

/// The range accepting overwrites is the half-written name — not an insertion at the caret, which
/// would leave `getCu` in front of the method it was the start of.
#[test]
fn accepting_replaces_the_half_written_name() {
    let p = cls("    private String customer;\n    getCu");
    let src = p.source("Order.java").to_string();
    let off = at(&src, "getCu") + "getCu".len();
    let hint = p.hint("Order.java", off).expect("a hint");
    assert_eq!(&src[hint.replace_start..hint.replace_end], "getCu");
}

/// Two candidates produce nothing, however close the second is. This is the whole difference
/// between the popup — where being wrong costs a keystroke — and the text drawn in the buffer.
#[test]
fn two_candidates_produce_no_ghost_text() {
    let p = cls("    private String customer;\n    private String custody;\n    getCust");
    assert!(hint_at(&p, "getCust").is_none());
    // …and the popup still offers both, which is what a popup is for.
    let labels = labels_at(&p, "getCust");
    assert!(labels.contains(&"getCustomer".to_string()), "{labels:?}");
    assert!(labels.contains(&"getCustody".to_string()), "{labels:?}");
}

/// One letter in a class with one field matches uniquely and means nothing — the caret has barely
/// arrived. A whole method appearing under it would be the editor guessing out loud.
#[test]
fn a_barely_started_name_is_not_certain_enough() {
    let p = cls("    private String customer;\n    ge");
    assert!(hint_at(&p, "\n    ge").is_none());
}

/// Everything the popup declines, the ghost text declines too — it is the same answer, held to a
/// stricter bar, not a second one computed another way.
#[test]
fn nothing_is_drawn_where_nothing_is_offered() {
    let inside_a_method = cls(
        "    private String customer;\n\
         \x20   void go() {\n\
         \x20       getCust\n\
         \x20   }",
    );
    assert!(hint_at(&inside_a_method, "getCust").is_none());

    let already_there = cls(
        "    private String customer;\n\
         \x20   public String getCustomer() { return customer; }\n\
         \x20   getCust",
    );
    assert!(hint_at(&already_there, "\n    getCust").is_none());
}

// ── The wither ───────────────────────────────────────────────────────────────────────────────

/// The builder-style setter — the same member the Generate dialog writes under "With", offered
/// where it is reached for.
#[test]
fn a_field_offers_a_wither_that_chains() {
    let p = cls("    private String customer;\n    withCu");
    let it = item_at(&p, "withCu", "withCustomer");
    let text = it.insert_text.expect("the member is the insertion");
    assert!(text.starts_with("public Order withCustomer(String customer) {"), "{text}");
    assert!(text.contains("return this;"), "{text}");
    // The row says what it gives back, which is the whole reason to reach for one.
    assert_eq!(it.detail.as_deref(), Some("(String) : Order"));
}

/// A `final` field is assigned once, at construction — nothing to chain.
#[test]
fn a_final_field_offers_no_wither() {
    let p = cls("    private final long id;\n    withI");
    assert!(!labels_at(&p, "withI").contains(&"withId".to_string()));
}

/// A `static` member has no `this` to return.
#[test]
fn a_static_field_offers_no_wither() {
    let p = cls("    private static int count;\n    withCo");
    assert!(!labels_at(&p, "withCo").contains(&"withCount".to_string()));
}

/// Named for the PROPERTY, like the setter: a boolean `isActive` chains as `withActive`.
#[test]
fn a_wither_takes_the_property_name() {
    let p = cls("    private boolean isActive;\n    withA");
    assert!(labels_at(&p, "withA").contains(&"withActive".to_string()));
}
