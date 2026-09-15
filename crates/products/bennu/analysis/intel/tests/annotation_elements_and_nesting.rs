//! Two ways a member key could name the wrong thing — both found by running the engine over a real
//! 1000-file project rather than over fixtures, and both of them silent.
//!
//! * An `@interface` element is a METHOD (JLS §9.6.1: annotation members compile to public abstract
//!   no-arg methods, and that is how the index records them). Classified as a field, it had no
//!   declaration the rename could edit and no uses recorded against it.
//! * Nested types were resolved by SIMPLE name, which is unique for top-level types and not for
//!   nested ones. Eleven test classes each declaring a nested `JakartaValidationTest` collapsed onto
//!   whichever binary won the map, so every caret in any of them answered about another file.

mod common;
use common::{at, Project};

const ANNOTATION: &str = r#"package p;
public @interface Customizer {
    boolean with_check() default false;
    String label() default "";
}
"#;

const USES_ANNOTATION: &str = r#"package p;
public class Handler {
    @Customizer(with_check = true)
    void guarded() {}

    boolean read(Customizer c) {
        return c.with_check();
    }
}
"#;

#[test]
fn an_annotation_element_renames_its_declaration_and_its_calls() {
    let p = Project::new(&[
        ("p/Customizer.java", ANNOTATION),
        ("p/Handler.java", USES_ANNOTATION),
    ]);
    let src = p.source("p/Customizer.java");
    let edits = p.rename_edits("p/Customizer.java", at(src, "with_check"), "withCheck");

    assert!(
        edits
            .iter()
            .any(|e| e.reason == bennu_intel::prelude::EditReason::Declaration),
        "no declaration edit — the element was not recognised as a member it can edit: {:?}",
        edits
            .iter()
            .map(|e| (e.file.as_str(), e.start, e.reason.label()))
            .collect::<Vec<_>>()
    );
    let uses = p.source("p/Handler.java");
    let call = at(uses, "c.with_check()") + "c.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file.ends_with("Handler.java") && e.start == call),
        "the call through the annotation instance was not renamed: {:?}",
        edits
            .iter()
            .map(|e| (e.file.as_str(), e.start))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_annotation_element_is_found_from_its_use_site() {
    let p = Project::new(&[
        ("p/Customizer.java", ANNOTATION),
        ("p/Handler.java", USES_ANNOTATION),
    ]);
    let uses = p.source("p/Handler.java");
    let found = p.find_usages("p/Handler.java", at(uses, "c.with_check()") + "c.".len());
    let label = found.map(|r| r.target.label()).unwrap_or_default();
    assert!(
        label.contains("method") && label.contains("with_check"),
        "an annotation element must classify as a method, got {label:?}"
    );
}

// Two files, each with a nested helper of the SAME simple name and a member of the same name in it.
// The simple-name map can only hold one of them.
const OUTER_A: &str = r#"package p;
public class AlphaTest {
    static class Helper {
        void run_case() {}
    }
    void go() { new Helper().run_case(); }
}
"#;

const OUTER_B: &str = r#"package p;
public class BetaTest {
    static class Helper {
        void run_case() {}
    }
    void go() { new Helper().run_case(); }
}
"#;

#[test]
fn same_named_nested_classes_keep_their_own_members() {
    let p = Project::new(&[("p/AlphaTest.java", OUTER_A), ("p/BetaTest.java", OUTER_B)]);
    let a = p.source("p/AlphaTest.java");
    let edits = p.rename_edits("p/AlphaTest.java", at(a, "run_case"), "runCase");

    assert!(
        edits
            .iter()
            .any(|e| e.reason == bennu_intel::prelude::EditReason::Declaration),
        "no declaration edit — the caret resolved to a type in another file: {:?}",
        edits
            .iter()
            .map(|e| (e.file.as_str(), e.start, e.reason.label()))
            .collect::<Vec<_>>()
    );
    assert!(
        edits.iter().all(|e| e.file.ends_with("AlphaTest.java")),
        "the rename reached the OTHER file's identically-named nested class: {:?}",
        edits.iter().map(|e| e.file.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn a_member_of_a_nested_class_is_keyed_under_the_enclosing_chain() {
    let p = Project::new(&[("p/AlphaTest.java", OUTER_A), ("p/BetaTest.java", OUTER_B)]);
    let b = p.source("p/BetaTest.java");
    let found = p.find_usages("p/BetaTest.java", at(b, "run_case"));
    let label = found.map(|r| r.target.label()).unwrap_or_default();
    assert!(
        label.contains("BetaTest"),
        "the member must belong to the nested class of THIS file, got {label:?}"
    );
}
