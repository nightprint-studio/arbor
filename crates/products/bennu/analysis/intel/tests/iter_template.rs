//! Autocompletion category — **the `iter` loop**.
//!
//! IntelliJ's `iter` loops over "the collection in scope", and that is why Arbor's built-in
//! abbreviations leave it out: a version that guesses writes a name that is not there. Here it is
//! offered only when a local or parameter in scope really is an array or an `Iterable`, and it
//! writes the very loop the postfix `.for` of that variable would. These tests pin both halves —
//! what it writes, and the places it must not appear.

mod common;
use bennu_proto::prelude::CompletionItem;
use common::*;

const ORDER: &str = "package shop;\npublic class Order { }\n";

/// `Cart.java` built from `marked` (the `¶` marks the caret and is removed), beside an `Order`
/// class; returns the project and the caret's offset.
fn cart(marked: &str) -> (Project, usize) {
    let offset = marked.find('¶').expect("a caret marker");
    let source = marked.replacen('¶', "", 1);
    (Project::new(&[("Order.java", ORDER), ("Cart.java", &source)]), offset)
}

fn offered(marked: &str) -> Vec<CompletionItem> {
    let (p, offset) = cart(marked);
    p.iter_complete("Cart.java", offset)
}

/// A method with `params` and `statements` as its body.
fn method(params: &str, statements: &str) -> String {
    format!("package shop;\npublic class Cart {{\n    void m({params}) {{\n{statements}\n    }}\n}}\n")
}

#[test]
fn an_array_in_scope_is_looped_over() {
    let items = offered(&method("Order[] orders", "        iter¶"));
    assert_eq!(items.len(), 1, "{items:?}");
    let text = items[0].insert_text.as_deref().expect("a body");
    assert!(text.starts_with("for (Order order : orders) {"), "{text}");
    assert!(items[0].snippet && items[0].preselect);
}

#[test]
fn the_word_is_replaced_by_the_loop() {
    let marked = method("Order[] orders", "        iter¶");
    let (p, offset) = cart(&marked);
    let item = &p.iter_complete("Cart.java", offset)[0];
    assert_eq!((item.replace_start, item.replace_end), (Some(offset - 4), Some(offset)));
}

/// With two candidates the nearest declaration wins, and the row says which — it is a choice, so it
/// is a popup row and not ghost text.
#[test]
fn the_variable_declared_nearest_the_caret_wins() {
    let marked = method("Order[] orders", "        Order[] pending = null;\n        iter¶");
    let item = &offered(&marked)[0];
    assert!(item.insert_text.as_deref().unwrap().contains(": pending)"), "{:?}", item.insert_text);
    assert_eq!(item.detail.as_deref(), Some("for each element of pending"));
}

#[test]
fn nothing_when_no_array_or_iterable_is_in_scope() {
    assert!(offered(&method("Order order, int n", "        iter¶")).is_empty());
}

#[test]
fn nothing_when_the_word_is_not_a_statement() {
    assert!(offered(&method("Order[] orders", "        run(iter¶")).is_empty(), "inside a call");
    assert!(offered(&method("Order[] orders", "        orders.iter¶")).is_empty(), "after a dot");
}

#[test]
fn nothing_for_a_word_that_is_not_iter() {
    assert!(offered(&method("Order[] orders", "        i¶")).is_empty(), "one letter");
    assert!(offered(&method("Order[] orders", "        iterate¶")).is_empty(), "a longer word");
}
