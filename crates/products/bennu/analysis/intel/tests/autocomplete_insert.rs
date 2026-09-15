//! Autocompletion category — **what accepting a candidate writes**.
//!
//! A completion list is only half of completion. The other half is the gesture: a method is a
//! call, and an editor that inserts only its name has left the two characters that make it one to
//! be typed by hand, on every method, every time. These tests are about the text that lands and
//! where the caret ends up in it — and about the case where adding it would break working code.

mod common;
use common::*;

fn tools() -> Project {
    Project::new(&[(
        "Tools.java",
        "package t;\n\
         public class Tools {\n\
         \x20   public int size() { return 0; }\n\
         \x20   public void put(String k, String v) { }\n\
         \x20   public String name;\n\
         \x20   public void use(Tools t) {\n\
         \x20       t.\n\
         \x20   }\n\
         \x20   public void again(Tools t) {\n\
         \x20       t.si()\n\
         \x20   }\n\
         }\n",
    )])
}

fn item(p: &Project, offset: usize, label: &str) -> bennu_proto::prelude::CompletionItem {
    p.complete("Tools.java", offset)
        .into_iter()
        .find(|i| i.label == label)
        .unwrap_or_else(|| panic!("no candidate {label:?}"))
}

fn at_dot(p: &Project) -> usize {
    let s = p.source("Tools.java");
    at(s, "t.\n") + "t.".len()
}

/// A no-argument call is finished the moment it lands: `size()`, caret after it, nothing to tab to.
#[test]
fn a_no_argument_method_inserts_its_parentheses_and_nothing_else() {
    let p = tools();
    let it = item(&p, at_dot(&p), "size");
    assert_eq!(it.insert_text.as_deref(), Some("size()"));
    assert!(it.snippet_stops.is_empty(), "{:?}", it.snippet_stops);
}

/// With something to pass, the caret goes BETWEEN the parentheses — which is also where the
/// signature popup is about to describe.
#[test]
fn a_method_with_parameters_parks_the_caret_between_the_parentheses() {
    let p = tools();
    let it = item(&p, at_dot(&p), "put");
    assert_eq!(it.insert_text.as_deref(), Some("put()"));
    assert_eq!(it.snippet_stops.len(), 1);
    let stop = &it.snippet_stops[0];
    assert_eq!(stop.start, "put(".len());
    assert_eq!(stop.start, stop.end, "a caret placement, not a selection");
}

/// A field is not a call. Nothing to insert beyond its name.
#[test]
fn a_field_inserts_its_name() {
    let p = tools();
    let it = item(&p, at_dot(&p), "name");
    assert!(it.insert_text.is_none(), "{:?}", it.insert_text);
}

/// `t.si|()` is correcting the name of a call that is already written. Inserting `size()` there
/// produces `size()()` — the completion breaking working code.
#[test]
fn a_call_that_is_already_written_keeps_its_own_parentheses() {
    let p = tools();
    let s = p.source("Tools.java").to_string();
    let off = at(&s, "t.si()") + "t.si".len();
    let it = p
        .complete("Tools.java", off)
        .into_iter()
        .find(|i| i.label == "size")
        .expect("size offered");
    assert!(it.insert_text.is_none(), "{:?}", it.insert_text);
}

/// A `(` on the NEXT line opens a different expression. Reading across the newline would leave
/// the parentheses off every candidate whose statement happens to be followed by a parenthesised
/// one — which is every line before an `if (`.
#[test]
fn a_parenthesis_on_the_next_line_is_a_different_expression() {
    let p = Project::new(&[(
        "Tools.java",
        "package t;\n\
         public class Tools {\n\
         \x20   public int size() { return 0; }\n\
         \x20   public void use(Tools t, boolean b) {\n\
         \x20       int n = t.si\n\
         \x20       if (b) { }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Tools.java").to_string();
    let off = at(&s, "t.si") + "t.si".len();
    let it = p
        .complete("Tools.java", off)
        .into_iter()
        .find(|i| i.label == "size")
        .expect("size offered");
    assert_eq!(it.insert_text.as_deref(), Some("size()"));
}

/// Every member candidate says which type declares it. It is what the popup draws as the item's
/// origin, and the handle its documentation is fetched by.
#[test]
fn a_member_carries_the_type_that_declares_it() {
    let p = tools();
    assert_eq!(item(&p, at_dot(&p), "size").owner.as_deref(), Some("t/Tools"));
}
