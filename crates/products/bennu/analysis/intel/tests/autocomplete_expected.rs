//! Autocompletion category — **the type the position wants**.
//!
//! Every other completion signal is about the candidate. This one is about the hole: after
//! `String name =` there are forty members on the receiver and a handful that can be written
//! there at all, and nothing about the receiver says which. These tests are about that term
//! reaching the ordering — and about it staying a *ranking* input, so a subtype that the
//! name comparison misses is still offered.

mod common;
use common::*;

/// One trigger per FILE. A `receiver.` with nothing after it is a syntax error, and tree-sitter's
/// recovery for several of them in one class body swallows the declarations between them — the
/// members would resolve, the enclosing type would not, and every case would answer empty for a
/// reason that has nothing to do with what is being tested.
fn shop(trigger: &str) -> Project {
    Project::new(&[
        (
            "Order.java",
            "package shop;\n\
             public class Order {\n\
             \x20   public String title() { return \"\"; }\n\
             \x20   public String vendor() { return \"\"; }\n\
             \x20   public int count() { return 0; }\n\
             \x20   public boolean paid() { return true; }\n\
             \x20   public void cancel() { }\n\
             }\n",
        ),
        (
            "Use.java",
            &format!(
                "package shop;\npublic class Use {{\n    public String run(Order o) {{\n        {trigger}\n    }}\n}}\n"
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

/// The headline. `String s = o.|` puts the two `String` methods above `count`, which cannot go
/// there at all.
#[test]
fn an_assignment_target_lifts_the_members_that_fit_it() {
    let p = shop("String s = o.");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o."));
    assert!(rank_of(&labels, "title") < rank_of(&labels, "count"), "{labels:?}");
    assert!(rank_of(&labels, "vendor") < rank_of(&labels, "count"), "{labels:?}");
}

/// `String s = o.cancel()` does not compile, and nothing else about `cancel` says so.
#[test]
fn a_void_method_sinks_where_a_value_is_wanted() {
    let p = shop("String s = o.");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o."));
    assert!(rank_of(&labels, "cancel") > rank_of(&labels, "count"), "{labels:?}");
}

/// Java's own autoboxing: `Integer n = o.count()` is legal, and a comparison that insisted on
/// the spelling would rank the one right answer nowhere.
#[test]
fn a_boxed_target_matches_the_primitive_that_fits_it() {
    let p = shop("Integer n = o.");
    let labels = p.complete_labels("Use.java", after(&p, "Integer n = o."));
    assert!(rank_of(&labels, "count") < rank_of(&labels, "title"), "{labels:?}");
}

/// A condition is a `boolean` — the one expected type that needs no resolving, and the one that
/// turns a member list into the predicates on it.
#[test]
fn a_condition_lifts_the_predicates() {
    let p = shop("if (o.) { }");
    let labels = p.complete_labels("Use.java", after(&p, "if (o."));
    assert!(rank_of(&labels, "paid") < rank_of(&labels, "title"), "{labels:?}");
}

#[test]
fn a_return_takes_the_methods_declared_type() {
    let p = shop("return o.");
    let labels = p.complete_labels("Use.java", after(&p, "return o."));
    assert!(rank_of(&labels, "title") < rank_of(&labels, "count"), "{labels:?}");
}

/// It ranks, it does not filter. `count` is wrong for a `String` and still offered — the
/// comparison is by name, so a genuine subtype is a miss, and hiding on a miss would hide the
/// right answer.
#[test]
fn a_member_that_does_not_fit_is_still_offered() {
    let p = shop("String s = o.");
    let labels = p.complete_labels("Use.java", after(&p, "String s = o."));
    assert!(labels.contains(&"count".to_string()), "{labels:?}");
    assert!(labels.contains(&"cancel".to_string()), "{labels:?}");
}

