//! What the PROJECT does with its members orders the popup: the method it calls all the time rises,
//! and the call it makes right after the previous one in a chain comes next.
//!
//! The census is counted from the project's own sources (the same function the index build uses) and
//! handed to the completion resolver, so what is asserted here is the whole path from "files that
//! call `zeta()`" to "`zeta` opens the list". Every name that must rise is alphabetically LAST among
//! its neighbours, so the assertion cannot pass by the tie-break the list falls back on.

mod common;
use common::*;

const REPO: &str = "package shop;\n\
     public class Repo {\n\
     \x20   public void alpha() {}\n\
     \x20   public void beta() {}\n\
     \x20   public void zeta() {}\n\
     }\n";

const HANDLE: &str = "package shop;\n\
     public class Handle {\n\
     \x20   public Handle abort() { return this; }\n\
     \x20   public Handle begin() { return this; }\n\
     \x20   public Handle commit() { return this; }\n\
     \x20   public Handle rollback() { return this; }\n\
     }\n";

/// `zeta` three times; `begin().rollback()` twice; `abort` and `commit` as often as `begin` and
/// `rollback`, so popularity alone cannot tell the four apart.
const CALLER: &str = "package shop;\n\
     class Caller {\n\
     \x20   void m(Repo r, Handle h) {\n\
     \x20       r.zeta();\n\
     \x20       r.zeta();\n\
     \x20       r.zeta();\n\
     \x20       h.begin().rollback();\n\
     \x20       h.begin().rollback();\n\
     \x20       h.abort();\n\
     \x20       h.abort();\n\
     \x20       h.commit();\n\
     \x20       h.commit();\n\
     \x20   }\n\
     }\n";

const USE: &str = "shop/Use.java";

const OURS: [&str; 7] = ["alpha", "beta", "zeta", "abort", "begin", "commit", "rollback"];

fn project_with(caller: &str, body: &str) -> Project {
    let user = format!(
        "package shop;\n\
         class Use {{\n\
         \x20   void m(Repo r, Handle h) {{\n\
         \x20       {body}\n\
         \x20   }}\n\
         }}\n"
    );
    Project::new(&[
        ("shop/Repo.java", REPO),
        ("shop/Handle.java", HANDLE),
        ("shop/Caller.java", caller),
        (USE, &user),
    ])
}

fn project(body: &str) -> Project {
    project_with(CALLER, body)
}

fn own_labels(p: &Project, needle: &str) -> Vec<String> {
    let caret = at_last(p.source(USE), needle) + needle.len();
    p.complete_labels(USE, caret)
        .into_iter()
        .filter(|l| OURS.contains(&l.as_str()))
        .collect()
}

#[test]
fn without_a_census_members_are_in_alphabetical_order() {
    let p = project("r.\n");
    assert_eq!(own_labels(&p, "r."), ["alpha", "beta", "zeta"]);
}

#[test]
fn the_method_the_project_calls_all_the_time_opens_the_list() {
    let p = project("r.\n").with_usage_census();
    assert_eq!(own_labels(&p, "r."), ["zeta", "alpha", "beta"]);
}

/// `h.begin().|` — the project rolls back what it begins, so `rollback` leads although it is
/// alphabetically last and exactly as popular as `abort`, `begin` and `commit`.
#[test]
fn the_call_the_project_makes_after_the_previous_one_comes_first() {
    let p = project("h.begin().\n").with_usage_census();
    let labels = own_labels(&p, "h.begin().");
    assert_eq!(labels.first().map(String::as_str), Some("rollback"), "{labels:?}");
}

/// The same position with no history for `begin`: nothing is ahead of the alphabet.
#[test]
fn a_sequence_the_project_never_writes_does_not_reorder_anything() {
    let p = project("h.commit().\n").with_usage_census();
    let labels = own_labels(&p, "h.commit().");
    assert_eq!(labels.first().map(String::as_str), Some("abort"), "{labels:?}");
}

#[test]
fn a_single_call_is_a_coincidence_and_changes_nothing() {
    let caller = "package shop;\nclass Caller { void m(Repo r) { r.zeta(); } }\n";
    let p = project_with(caller, "r.\n").with_usage_census();
    assert_eq!(own_labels(&p, "r."), ["alpha", "beta", "zeta"]);
}
