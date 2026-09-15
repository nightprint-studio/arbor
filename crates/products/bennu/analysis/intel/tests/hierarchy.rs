//! Call and type hierarchy over the real engine.
//!
//! The two call directions are the reference index read two ways, so what these pin is mostly that
//! the reading agrees with find-usages: the same edges, attributed to the declaration they sit in.
//! The interesting cases are the ones where "the method's own bucket" is not the whole answer —
//! a call written against an interface, a call inside an anonymous class — and the ones where a
//! naive reading would produce a wrong row.

mod common;
use common::{at, Project};

use bennu_intel::prelude::{HierarchyDirection::*, HierarchyItem};

const SERVICE: &str = r#"package p;
public interface Service {
    void run(String job);
}
"#;

const IMPL: &str = r#"package p;
public class Impl implements Service {
    private final Audit audit = new Audit();
    @Override
    public void run(String job) {
        audit.note(job);
        audit.note(job);
    }
}
"#;

const AUDIT: &str = r#"package p;
public class Audit {
    private String last = describe();
    public void note(String what) {}
    static String describe() { return "audit"; }
}
"#;

const CALLER: &str = r#"package p;
public class Caller {
    void viaInterface(Service s) {
        s.run("a");
    }
    void viaImpl(Impl i) {
        i.run("b");
    }
}
"#;

fn project() -> Project {
    Project::new(&[
        ("Service.java", SERVICE),
        ("Impl.java", IMPL),
        ("Audit.java", AUDIT),
        ("Caller.java", CALLER),
    ])
}

/// The row labels of a level, so an assertion reads like the panel does.
fn names(items: &[HierarchyItem]) -> Vec<String> {
    items.iter().map(|i| i.name.clone()).collect()
}

/// The single root of the call hierarchy for the caret on `needle` in `file`.
fn call_root(p: &Project, file: &str, needle: &str) -> HierarchyItem {
    let src = p.source(file).to_string();
    let mut roots = p.call_hierarchy(file, at(&src, needle));
    assert_eq!(roots.len(), 1, "expected one root for {needle:?}, got {:?}", names(&roots));
    roots.remove(0)
}

#[test]
fn a_root_is_the_method_the_caret_is_on() {
    let p = project();
    let root = call_root(&p, "Audit.java", "note(String what)");
    assert_eq!(root.name, "note(String)");
    assert_eq!(root.kind, "method");
    assert_eq!(root.detail.as_deref(), Some("Audit"));
    assert_eq!(root.file, "Audit.java");
}

#[test]
fn callers_are_grouped_by_the_method_they_sit_in() {
    let p = project();
    let root = call_root(&p, "Audit.java", "note(String what)");
    let callers = p.hierarchy_step(&root, Incoming);
    assert_eq!(names(&callers), vec!["run(String)"]);
    // Two calls inside one method are ONE row carrying both, not two rows — which is what the
    // panel's `2×` badge reads.
    assert_eq!(callers[0].call_sites.len(), 2);
}

#[test]
fn a_call_written_against_the_interface_is_a_caller_of_the_implementation() {
    let p = project();
    // The caret is on the implementation. A call site written as `s.run(…)` on a `Service` sits in
    // the interface's bucket, not this one — and it is still a caller of this method.
    let root = call_root(&p, "Impl.java", "run(String job)");
    let callers = p.hierarchy_step(&root, Incoming);
    let mut labels: Vec<String> = callers
        .iter()
        .map(|c| format!("{}.{}", c.detail.clone().unwrap_or_default(), c.name))
        .collect();
    labels.sort();
    assert_eq!(labels, vec!["Caller.viaImpl(Impl)", "Caller.viaInterface(Service)"]);
}

#[test]
fn callees_are_what_the_body_reaches() {
    let p = project();
    let root = call_root(&p, "Impl.java", "run(String job)");
    let callees = p.hierarchy_step(&root, Outgoing);
    assert_eq!(names(&callees), vec!["note(String)"]);
    // The row points at the callee's declaration, and carries the sites in the caller's body.
    assert_eq!(callees[0].file, "Audit.java");
    assert_eq!(callees[0].call_sites.len(), 2);
}

#[test]
fn a_use_site_in_a_field_initialiser_is_attributed_to_its_type() {
    let p = project();
    // `describe()` is called from a field initialiser — there is no enclosing method to name.
    let root = call_root(&p, "Audit.java", "describe() { return");
    let callers = p.hierarchy_step(&root, Incoming);
    assert_eq!(names(&callers), vec!["Audit"]);
    assert_eq!(callers[0].kind, "class");
    // And it is a leaf: what "calls" a field initialiser is a constructor, which the index does
    // not record as an edge — claiming otherwise would be an invention.
    assert!(p.hierarchy_step(&callers[0], Incoming).is_empty());
}

#[test]
fn a_caret_on_a_type_is_not_a_call_hierarchy() {
    let p = project();
    let src = p.source("Impl.java").to_string();
    assert!(p.call_hierarchy("Impl.java", at(&src, "Impl implements") ).is_empty());
}

#[test]
fn implementors_are_the_subtypes() {
    let p = project();
    let src = p.source("Service.java").to_string();
    let mut roots = p.type_hierarchy("Service.java", at(&src, "Service {"));
    assert_eq!(names(&roots), vec!["Service"]);
    let root = roots.remove(0);
    assert_eq!(root.kind, "interface");
    assert_eq!(names(&p.hierarchy_step(&root, Subtypes)), vec!["Impl"]);
}

#[test]
fn supertypes_are_what_the_type_is_built_on() {
    let p = project();
    let src = p.source("Impl.java").to_string();
    let root = p.type_hierarchy("Impl.java", at(&src, "Impl implements")).remove(0);
    assert_eq!(names(&p.hierarchy_step(&root, Supertypes)), vec!["Service"]);
}

#[test]
fn a_caret_on_a_member_opens_its_owners_type_hierarchy() {
    let p = project();
    let src = p.source("Impl.java").to_string();
    // Deliberately in the middle of a method rather than on the class name: Ctrl+H while reading a
    // body is asking about the class you are reading.
    let roots = p.type_hierarchy("Impl.java", at(&src, "run(String job)"));
    assert_eq!(names(&roots), vec!["Impl"]);
}

#[test]
fn a_direction_that_does_not_apply_to_the_node_answers_nothing() {
    let p = project();
    let src = p.source("Service.java").to_string();
    let type_root = p.type_hierarchy("Service.java", at(&src, "Service {")).remove(0);
    // The panel keeps its roots when the direction chips change, so a type node CAN be asked a call
    // question. Guessing which of the four was meant would hang the wrong list under it.
    assert!(p.hierarchy_step(&type_root, Incoming).is_empty());
    assert!(p.hierarchy_step(&type_root, Outgoing).is_empty());
}
