//! Name affinity — a candidate that says the same word as the thing being filled.
//!
//! In every case here the TYPE cannot decide: the candidates all produce a `String` (or all a
//! `Customer`), so the type-based ordering ties and what is left is the alphabet. The name being
//! filled is what lifts the right one — which is the whole reason the signal exists.
//!
//! The tuning is process-wide and nothing here touches it; the switch and the strength are unit
//! tested where a tuning can be handed to the ranking directly.

mod common;
use common::*;

fn domain(trigger: &str) -> Project {
    Project::new(&[
        (
            "Customer.java",
            "package shop;\n\
             public class Customer {\n\
             \x20   public String email() { return \"\"; }\n\
             \x20   public String name() { return \"\"; }\n\
             \x20   public String phone() { return \"\"; }\n\
             }\n",
        ),
        (
            "Use.java",
            &format!(
                "package shop;\npublic class Use {{\n    private Customer winner;\n    \
                 public void run(Customer alpha, Customer winnerCandidate, Customer c) {{\n        {trigger}\n    }}\n}}\n"
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

/// Alphabetically `email` comes first; the declaration `String name = c.|` says `name`.
#[test]
fn a_declaration_lifts_the_member_with_its_name() {
    let p = domain("String name = c.");
    let labels = p.complete_labels("Use.java", after(&p, "String name = c."));
    assert!(rank_of(&labels, "name") < rank_of(&labels, "email"), "{labels:?}");
    assert!(rank_of(&labels, "name") < rank_of(&labels, "phone"), "{labels:?}");
}

#[test]
fn an_assignment_target_lifts_the_member_with_its_name() {
    let p = domain("String phone;\n        phone = c.");
    let labels = p.complete_labels("Use.java", after(&p, "phone = c."));
    assert!(rank_of(&labels, "phone") < rank_of(&labels, "email"), "{labels:?}");
}

/// A getter is read through its verb: `getName` is `name`. (Here the members are accessors named
/// the plain way, so this pins the other direction — a hole called `customerName` echoes `name`.)
#[test]
fn a_longer_hole_name_still_shares_its_last_word() {
    let p = domain("String customerName = c.");
    let labels = p.complete_labels("Use.java", after(&p, "String customerName = c."));
    assert!(rank_of(&labels, "name") < rank_of(&labels, "email"), "{labels:?}");
}

/// The same signal for a bare name: among three equally-typed parameters the one that echoes the
/// field being assigned leads — `alpha` would otherwise win the alphabet.
#[test]
fn a_bare_name_lifts_the_binding_that_echoes_the_target() {
    let p = domain("this.winner = ");
    let labels = p.scope_labels("Use.java", after(&p, "this.winner = "));
    assert!(rank_of(&labels, "winnerCandidate") < rank_of(&labels, "alpha"), "{labels:?}");
}

/// An argument slot is named by its callee: nothing about the parameter's type says `c`, but
/// `registerWinner(` does for `winnerCandidate`.
#[test]
fn an_argument_is_named_by_its_callee() {
    let p = domain("registerWinner(");
    let labels = p.scope_labels("Use.java", after(&p, "registerWinner("));
    assert!(rank_of(&labels, "winnerCandidate") < rank_of(&labels, "alpha"), "{labels:?}");
}

/// Nothing is hidden by it: the members that do not echo the name are still offered.
#[test]
fn affinity_ranks_and_never_filters() {
    let p = domain("String name = c.");
    let labels = p.complete_labels("Use.java", after(&p, "String name = c."));
    for member in ["email", "name", "phone"] {
        assert!(labels.contains(&member.to_string()), "{member} missing from {labels:?}");
    }
}
