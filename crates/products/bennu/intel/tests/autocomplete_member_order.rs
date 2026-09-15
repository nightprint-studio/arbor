//! The order and the reading of a member list, IntelliJ's way — the reported popup after `route.`:
//!
//! ```java
//! public record ServiceRoute(String prefix, String target_uri) { … }
//!
//! void handle(ServiceRoute route) { route.| }
//! ```
//!
//! It opened on the record's implicit `equals` and `hashCode`, interleaved alphabetically with
//! `prefix` and `target_uri`, offered `Object`'s protected `clone` and `finalize`, and folded the
//! overloads into one row. What the author reaches for nine times in ten is what the record declares.
//!
//! One trigger per file: a half-written member access is a syntax error, and several in one class
//! body let tree-sitter's recovery swallow the declarations between them.

mod common;
use common::*;

use bennu_proto::prelude::MemberOrigin;

const ROUTE: &str = "package com.acme.route;\n\
     public record ServiceRoute(String prefix, String target_uri) {\n\
     \x20   public boolean matches(String path) { return path != null; }\n\
     \x20   public boolean matches(String path, boolean exact) { return exact; }\n\
     }\n";

const HANDLER: &str = "com/acme/web/Handler.java";

/// The route in its package, and a handler in ANOTHER package with `body` as its method body.
fn project(body: &str) -> Project {
    let handler = format!(
        "package com.acme.web;\n\
         import com.acme.route.ServiceRoute;\n\
         public class Handler {{\n\
         \x20   void handle(ServiceRoute route) {{\n\
         \x20       {body}\n\
         \x20   }}\n\
         }}\n"
    );
    Project::new(&[("com/acme/route/ServiceRoute.java", ROUTE), (HANDLER, &handler)])
}

fn after(p: &Project, needle: &str) -> usize {
    at_last(p.source(HANDLER), needle) + needle.len()
}

#[test]
fn the_receivers_own_members_come_first_then_the_implicit_ones_then_objects() {
    let p = project("route.\n");
    let labels = p.complete_labels(HANDLER, after(&p, "route."));
    assert_eq!(
        labels,
        [
            // Declared by the record — the overloads as two rows, fewest parameters first.
            "matches", "matches", "prefix", "target_uri",
            // Implemented for the record by the compiler.
            "equals", "hashCode", "toString",
            // `java.lang.Object`'s, without its protected members.
            "getClass",
        ],
    );
}

#[test]
fn every_member_says_where_it_stands() {
    let p = project("route.\n");
    let items = p.complete(HANDLER, after(&p, "route."));
    let origin_of = |label: &str| items.iter().find(|i| i.label == label).and_then(|i| i.member_origin);
    assert_eq!(origin_of("prefix"), Some(MemberOrigin::Own));
    assert_eq!(origin_of("equals"), Some(MemberOrigin::Inherited));
    assert_eq!(origin_of("getClass"), Some(MemberOrigin::Object));
}

/// `route.clone()` from `Handler` is not Java: `clone` is protected and `Handler` is not a
/// `ServiceRoute`.
#[test]
fn objects_protected_members_are_not_offered_through_another_classes_receiver() {
    let p = project("route.\n");
    let labels = p.complete_labels(HANDLER, after(&p, "route."));
    for protected in ["clone", "finalize"] {
        assert!(!labels.contains(&protected.to_string()), "{protected}: {labels:?}");
    }
}

/// Through its own type a class does inherit them.
#[test]
fn a_class_is_offered_the_protected_members_it_inherits_through_this() {
    let p = project("this.\n");
    let labels = p.complete_labels(HANDLER, after(&p, "this."));
    assert!(labels.contains(&"clone".to_string()), "{labels:?}");
}

#[test]
fn the_first_own_member_is_preselected_right_after_the_dot() {
    let p = project("route.\n");
    let items = p.complete(HANDLER, after(&p, "route."));
    assert!(items[0].preselect, "{:?}", items[0]);
    assert!(items.iter().skip(1).all(|i| !i.preselect));
}

/// The row reads `matches(String path)` · `boolean`, and each overload shows its own parameters.
#[test]
fn a_method_row_carries_its_named_parameters_and_its_return_type() {
    let p = project("route.\n");
    let items = p.complete(HANDLER, after(&p, "route."));
    let matches: Vec<(Option<&str>, Option<&str>)> = items
        .iter()
        .filter(|i| i.label == "matches")
        .map(|i| (i.signature.as_deref(), i.detail.as_deref()))
        .collect();
    assert_eq!(
        matches,
        [(Some("(String path)"), Some("boolean")), (Some("(String path, boolean exact)"), Some("boolean"))],
    );
}

/// Declared `Class<?>`, typed by the compiler after the receiver — which is what IntelliJ shows.
#[test]
fn get_class_is_typed_after_the_receiver() {
    let p = project("route.\n");
    let items = p.complete(HANDLER, after(&p, "route."));
    let get_class = items.iter().find(|i| i.label == "getClass").expect("getClass offered");
    assert_eq!(get_class.detail.as_deref(), Some("Class<? extends ServiceRoute>"));
}
