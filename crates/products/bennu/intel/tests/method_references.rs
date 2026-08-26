//! `Type::method` and `expr::method` — method references.
//!
//! The reference walk recorded call sites and never these, so a rename moved the declaration and
//! every `foo.bar()` while leaving `Foo::bar` spelling the old name.

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
