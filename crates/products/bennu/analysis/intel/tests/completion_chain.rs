//! Chain completion — members of members that produce the type the position wants.
//!
//! `String s = o.|` has `title()` as a direct answer and `customer().name()` a hop away. These tests
//! pin that the hop appears, where it sits relative to the direct members, and the cases where it
//! must stay out: nothing expected, a call that is already written, a first hop that already fits.
//!
//! The ranking tuning is a process-wide value, so nothing here changes it: the default (everything
//! on) is what is under test, and the switches are covered by the unit tests, which hand a tuning to
//! the ranking directly.

mod common;
use common::*;

fn domain(trigger: &str) -> Project {
    Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             public class Order {\n\
             \x20   public String title() { return \"\"; }\n\
             \x20   public int count() { return 0; }\n\
             \x20   public Customer customer() { return null; }\n\
             \x20   public Money total() { return null; }\n\
             }\n",
        ),
        (
            "Customer.java",
            "package shop;\n\
             public class Customer {\n\
             \x20   public String name() { return \"\"; }\n\
             \x20   public String email() { return \"\"; }\n\
             \x20   public Address address() { return null; }\n\
             }\n",
        ),
        (
            "Address.java",
            "package shop;\npublic class Address {\n    public String city() { return \"\"; }\n}\n",
        ),
        (
            "Money.java",
            "package shop;\npublic class Money {\n    public String currency() { return \"\"; }\n}\n",
        ),
        (
            "Use.java",
            &format!(
                "package shop;\npublic class Use {{\n    public void run(Order o) {{\n        {trigger}\n    }}\n}}\n"
            ),
        ),
    ])
}

fn after(p: &Project, needle: &str) -> usize {
    at(p.source("Use.java"), needle) + needle.len()
}

fn rank_of(labels: &[String], name: &str) -> usize {
    labels.iter().position(|l| l == name).unwrap_or(usize::MAX)
}

#[test]
fn a_member_of_a_member_that_produces_the_expected_type_is_offered() {
    let p = domain("String s = o.");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o."));
    assert!(labels.contains(&"customer().name()".to_string()), "{labels:?}");
    assert!(labels.contains(&"customer().email()".to_string()), "{labels:?}");
    assert!(labels.contains(&"total().currency()".to_string()), "{labels:?}");
}

/// The direct answer stays first: a chain is the second thing to look at, not the first.
#[test]
fn chains_sit_below_the_members_that_fit_directly_and_above_the_ones_that_do_not() {
    let p = domain("String s = o.");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o."));
    let title = rank_of(&labels, "title");
    let chain = rank_of(&labels, "customer().name()");
    let miss = rank_of(&labels, "count");
    assert!(title < chain, "the direct fit leads: {labels:?}");
    assert!(chain < miss, "a chain is nearer than a member that cannot be written: {labels:?}");
}

#[test]
fn the_chain_is_inserted_as_a_whole_call_sequence() {
    let p = domain("String s = o.");
    let items = p.complete("Use.java", after(&p, "String s = o."));
    let chain = items.iter().find(|i| i.label == "customer().name()").expect("a chain row");
    assert_eq!(chain.insert_text.as_deref(), Some("customer().name()"));
    assert!(chain.snippet_stops.is_empty());
}

/// A chain only exists relative to something wanted. `o.|` as a bare statement wants nothing.
#[test]
fn nothing_is_chained_when_the_position_wants_nothing() {
    let p = domain("o.");
    let labels = p.complete_labels("Use.java", after(&p, "o."));
    assert!(labels.iter().all(|l| !l.contains('.')), "{labels:?}");
}

/// `customer()` returns a `Customer` — the very type wanted — so it is a row of its own, and no
/// chain through it is offered beside it.
#[test]
fn a_first_hop_that_already_fits_is_not_chained_through() {
    let p = domain("Customer c = o.");
    let labels = p.complete_labels("Use.java", after(&p, "Customer c = o."));
    assert!(labels.contains(&"customer".to_string()), "{labels:?}");
    assert!(labels.iter().all(|l| !l.starts_with("customer().")), "{labels:?}");
}

/// `o.ti|()` is a call being corrected: a chain cannot replace half of it.
#[test]
fn a_call_that_is_already_written_gets_no_chains() {
    let p = domain("String s = o.cu();");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o.cu"));
    assert!(labels.iter().all(|l| !l.contains('.')), "{labels:?}");
}

#[test]
fn the_typed_prefix_narrows_chains_by_their_first_hop() {
    let p = domain("String s = o.cus");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o.cus"));
    assert!(labels.contains(&"customer().name()".to_string()), "{labels:?}");
    assert!(labels.iter().all(|l| !l.starts_with("total()")), "{labels:?}");
}

/// A second hop of a different type is not what is wanted: `Address` is not a `String`.
#[test]
fn only_the_second_hop_that_produces_the_type_is_offered() {
    let p = domain("String s = o.");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o."));
    assert!(!labels.contains(&"customer().address()".to_string()), "{labels:?}");
}

/// Chains are added by hand below the fits and must not take the preselection, which belongs to
/// the single exact fit among the direct members.
#[test]
fn chains_never_take_the_preselection() {
    let p = domain("Customer c = o.");
    let items = p.complete("Use.java", after(&p, "Customer c = o."));
    assert!(items.iter().filter(|i| i.label.contains('.')).all(|i| !i.preselect));
}
