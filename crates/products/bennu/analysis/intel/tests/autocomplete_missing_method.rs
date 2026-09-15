//! Autocompletion — **the method this class calls and has not declared**.
//!
//! You write `randomico()` inside a method, then go to the class body and type `rand`. What should
//! appear is an offer to declare it, with the signature the call already specified.
//!
//! What appeared instead was the word `randomico` scraped out of the buffer by a regular
//! expression, because the only thing that knew the method was missing was the **diagnostic** — and
//! a half-typed name in a class body is a syntax error, so the whole file stops validating at
//! exactly the keystroke where the offer is wanted. That guard is right (`bennu-check` returns the
//! syntax error and nothing else, deliberately). So the offer is read from the TREE, where the call
//! is still sitting beside the ERROR node the half-written name recovered as.

mod common;
use common::*;
use bennu_proto::prelude::CompletionItem;

/// A class that calls something it does not declare, with the caret half-way through writing it.
fn calling(body: &str, typed: &str) -> Project {
    Project::new(&[
        (
            "Base.java",
            "package p;\npublic class Base {\n    protected void inherited() { }\n}\n",
        ),
        (
            "Order.java",
            &format!(
                "package p;\npublic class Order extends Base {{\n    public void go() {{\n{body}\n    }}\n\n    {typed}\n}}\n"
            ),
        ),
    ])
}

fn generated_at(p: &Project, needle: &str) -> Vec<CompletionItem> {
    let s = p.source("Order.java").to_string();
    let off = at_last(&s, needle) + needle.len();
    p.generated("Order.java", off)
}

fn labels(p: &Project, needle: &str) -> Vec<String> {
    generated_at(p, needle).into_iter().map(|i| i.label).collect()
}

// ── The offer ────────────────────────────────────────────────────────────────────────────────

/// The reported case, end to end.
#[test]
fn a_method_the_class_calls_and_does_not_declare_is_offered() {
    let p = calling("        randomico();", "rand");
    assert!(labels(&p, "rand").contains(&"randomico".to_string()), "{:?}", labels(&p, "rand"));
}

/// The call site is the specification, and it is the same reading the Alt+Enter fix makes — so the
/// popup and the quick fix cannot describe one member two ways.
#[test]
fn the_call_site_writes_the_signature() {
    let p = calling("        int d = randomico(\"ciao\", 3);", "rand");
    let it = generated_at(&p, "rand")
        .into_iter()
        .find(|i| i.label == "randomico")
        .expect("offered");
    assert_eq!(it.detail.as_deref(), Some("(String, int) : int"));
    let text = it.insert_text.expect("the member is the insertion");
    assert!(text.starts_with("private int randomico(String arg1, int arg2) {"), "{text}");
    assert!(text.contains("throw new UnsupportedOperationException"), "{text}");
    assert_eq!(it.kind, "generate");
}

/// An argument that is a NAME lends it to the parameter — the one piece of naming the call site
/// actually knows.
#[test]
fn an_argument_that_is_a_name_lends_it() {
    let p = calling("        String label = \"x\";\n        randomico(label);", "rand");
    let it = generated_at(&p, "rand")
        .into_iter()
        .find(|i| i.label == "randomico")
        .expect("offered");
    assert!(
        it.insert_text.unwrap().contains("randomico(String label)"),
        "the parameter keeps the argument's name"
    );
}

/// A call from a `static` method must reach a `static` one, or the stub does not compile at the
/// only site that asked for it.
#[test]
fn a_call_from_a_static_method_asks_for_a_static_one() {
    let p = Project::new(&[(
        "Order.java",
        "package p;\npublic class Order {\n    public static void main(String[] a) {\n        randomico();\n    }\n\n    rand\n}\n",
    )]);
    let s = p.source("Order.java").to_string();
    let off = at_last(&s, "rand") + "rand".len();
    let it = p
        .generated("Order.java", off)
        .into_iter()
        .find(|i| i.label == "randomico")
        .expect("offered");
    assert!(it.insert_text.unwrap().starts_with("private static void randomico()"));
}

/// The humps reach it, like every other candidate.
#[test]
fn the_humps_reach_an_undeclared_call() {
    let p = calling("        calcolaTotale();", "cT");
    assert!(labels(&p, "cT").contains(&"calcolaTotale".to_string()));
}

// ── Where it must stay quiet ─────────────────────────────────────────────────────────────────

/// Offering to write a method a supertype already declares is offering to break the build.
#[test]
fn an_inherited_method_is_not_offered_as_missing() {
    let p = calling("        inherited();", "inh");
    assert!(!labels(&p, "inh").contains(&"inherited".to_string()), "{:?}", labels(&p, "inh"));
}

/// A method the class DOES declare is not missing.
#[test]
fn a_method_the_class_declares_is_not_offered() {
    let p = Project::new(&[(
        "Order.java",
        "package p;\npublic class Order {\n    void go() { helper(); }\n    void helper() { }\n\n    help\n}\n",
    )]);
    let s = p.source("Order.java").to_string();
    let off = at_last(&s, "help") + "help".len();
    assert!(p
        .generated("Order.java", off)
        .into_iter()
        .all(|i| i.label != "helper"));
}

/// A call on another object belongs in that object's class — `create-method-in`'s question, not
/// this one.
#[test]
fn a_call_on_another_object_is_not_offered_here() {
    let p = Project::new(&[(
        "Order.java",
        "package p;\npublic class Order {\n    void go(String s) { s.randomico(); }\n\n    rand\n}\n",
    )]);
    let s = p.source("Order.java").to_string();
    let off = at_last(&s, "rand") + "rand".len();
    assert!(p
        .generated("Order.java", off)
        .into_iter()
        .all(|i| i.label != "randomico"));
}

/// Inside a method body `randomico` is a call, not a declaration.
#[test]
fn nothing_is_offered_inside_a_method_body() {
    let p = Project::new(&[(
        "Order.java",
        "package p;\npublic class Order {\n    void go() {\n        randomico();\n        rand\n    }\n}\n",
    )]);
    let s = p.source("Order.java").to_string();
    let off = at_last(&s, "rand") + "rand".len();
    assert!(p.generated("Order.java", off).is_empty());
}

/// Two call sites of the same undeclared method are ONE member to write.
#[test]
fn the_same_name_is_offered_once() {
    let p = calling("        randomico();\n        randomico();", "rand");
    let n = labels(&p, "rand").iter().filter(|l| *l == "randomico").count();
    assert_eq!(n, 1);
}

// ── Ghost text ───────────────────────────────────────────────────────────────────────────────

#[test]
fn one_missing_method_is_drawn_ahead_of_the_caret() {
    let p = calling("        randomico();", "rand");
    let s = p.source("Order.java").to_string();
    let off = at_last(&s, "rand") + "rand".len();
    let hint = p.hint("Order.java", off).expect("a hint");
    assert!(hint.insert.contains("private void randomico()"), "{}", hint.insert);
}
