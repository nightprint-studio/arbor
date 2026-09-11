//! `Type::method` and `expr::method` — method references.
//!
//! The reference walk recorded call sites and never these, so a rename moved the declaration and
//! every `foo.bar()` while leaving `Foo::bar` spelling the old name. Once it did, the caret
//! classifier go-to, find-usages and hover share still had no arm for them: all three answered
//! nothing with the caret on a use the index had counted.

mod common;
use common::{at, Project};

const SRC: &str = r#"package p;
public class Reports {
    public record Failure(String source_path) {}

    String describe(Failure f) {
        return f.source_path();
    }

    java.util.function.Function<Failure, String> byRef() {
        return Failure::source_path;
    }

    static String helper(String s) { return s; }

    java.util.function.Function<String, String> staticRef() {
        return Reports::helper;
    }

    String twice(String s) { return s + s; }

    java.util.function.UnaryOperator<String> boundRef() {
        return this::twice;
    }
}
"#;

fn project() -> Project {
    Project::new(&[("p/Reports.java", SRC)])
}

#[test]
fn a_method_reference_to_a_record_accessor_is_renamed() {
    let p = project();
    let src = p.source("p/Reports.java");
    let edits = p.rename_edits("p/Reports.java", at(src, "source_path"), "sourcePath");
    let at_ref = at(src, "Failure::source_path") + "Failure::".len();
    assert!(
        edits.iter().any(|e| e.start == at_ref),
        "the method reference was not renamed; edits: {:?}",
        edits
            .iter()
            .map(|e| (e.start, e.reason.label()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_method_reference_to_a_static_method_is_renamed() {
    let p = project();
    let src = p.source("p/Reports.java");
    let edits = p.rename_edits(
        "p/Reports.java",
        at(src, "static String helper") + "static String ".len(),
        "convert",
    );
    let at_ref = at(src, "Reports::helper") + "Reports::".len();
    assert!(
        edits.iter().any(|e| e.start == at_ref),
        "the static method reference was not renamed; edits: {:?}",
        edits
            .iter()
            .map(|e| (e.start, e.reason.label()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn find_usages_counts_the_method_reference() {
    let p = project();
    let src = p.source("p/Reports.java");
    // Asked of the ACCESSOR — a record component is a field and an accessor, two distinct keys, and
    // the call and the reference are uses of the accessor.
    let call = at(src, "f.source_path()") + "f.".len();
    let n = p.usage_count("p/Reports.java", call);
    assert_eq!(n, 2, "expected the call and the method reference");
}

#[test]
fn go_to_from_a_method_reference_lands_on_the_static_method() {
    let p = project();
    let src = p.source("p/Reports.java");
    let d = p
        .goto("p/Reports.java", at(src, "Reports::helper") + "Reports::".len())
        .expect("the method reference resolves");
    assert_eq!(d.start, at(src, "static String helper") + "static String ".len());
}

#[test]
fn go_to_from_a_method_reference_to_a_record_accessor_lands_on_the_component() {
    let p = project();
    let src = p.source("p/Reports.java");
    let d = p
        .goto("p/Reports.java", at(src, "Failure::source_path") + "Failure::".len())
        .expect("the accessor reference resolves");
    assert_eq!(d.start, at(src, "String source_path") + "String ".len());
}

/// Qualified by a VALUE, not a type: the receiver is typed, not looked up by name.
#[test]
fn go_to_from_a_bound_method_reference_lands_on_the_method() {
    let p = project();
    let src = p.source("p/Reports.java");
    let d = p
        .goto("p/Reports.java", at(src, "this::twice") + "this::".len())
        .expect("the bound reference resolves");
    assert_eq!(d.start, at(src, "String twice") + "String ".len());
}

/// The qualifier is a use of the type — not a field of the class the reference is written in,
/// which is what the bare-identifier fallback would have made of it.
#[test]
fn go_to_on_the_qualifier_of_a_method_reference_opens_the_type() {
    let p = project();
    let src = p.source("p/Reports.java");
    assert_eq!(
        p.goto_label("p/Reports.java", at(src, "Reports::helper")).as_deref(),
        Some("class p.Reports"),
    );
}

#[test]
fn find_usages_asked_from_the_method_reference_itself() {
    let p = project();
    let src = p.source("p/Reports.java");
    let n = p.usage_count("p/Reports.java", at(src, "Reports::helper") + "Reports::".len());
    assert_eq!(n, 1, "the reference is the one use of `helper`");
}
