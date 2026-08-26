//! Renaming a member whose USE SITE is reached through a LIBRARY generic.
//!
//! The regression these lock down: a library type is not only a destination, it is a **conduit**.
//! In `failures.stream().map(f -> f.source_path())` the lambda parameter `f` is a PROJECT type that
//! only arrives by substitution through `List<E>` → `Stream<T>` → `Function<T, R>`. When the
//! reference walk could not resolve those, `f` stayed untyped, no edge was recorded for
//! `f.source_path()`, and renaming the component silently skipped every such call — the rename
//! looked like it worked (declaration + plain call sites) while leaving the stream chains broken.
//!
//! `Project::with_stream_jdk` lends the engine a resolver that can see those three types, which is
//! what the provider does in production.

mod common;
use common::{at, Project};

/// The user's real shape: a record nested inside the class that streams over it.
const NESTED_RECORD_STREAM: &str = r#"package it.report;
import java.util.List;

public class JSRxmlCompiler {
    public record CompilationFailure(String source_path, String message) {}

    String describe(List<CompilationFailure> failures) {
        return failures.stream()
            .map(failure -> failure.source_path())
            .findFirst();
    }

    String direct(CompilationFailure f) {
        return f.source_path();
    }
}
"#;

fn project() -> Project {
    Project::with_stream_jdk(&[("it/report/JSRxmlCompiler.java", NESTED_RECORD_STREAM)])
}

const FILE: &str = "it/report/JSRxmlCompiler.java";

#[test]
fn renaming_a_record_component_reaches_the_accessor_inside_a_stream_lambda() {
    let p = project();
    let src = p.source(FILE);
    let edits = p.rename_edits(FILE, at(src, "source_path"), "sourcePath");

    let in_lambda = at(src, "failure.source_path()") + "failure.".len();
    assert!(
        edits.iter().any(|e| e.start == in_lambda),
        "the accessor call inside the stream lambda was not renamed; edits: {:?}",
        edits.iter().map(|e| (e.start, e.reason.label())).collect::<Vec<_>>()
    );
}

#[test]
fn the_declaration_and_the_plain_call_site_come_too() {
    let p = project();
    let src = p.source(FILE);
    let edits = p.rename_edits(FILE, at(src, "source_path"), "sourcePath");

    assert!(
        edits.iter().any(|e| e.reason.label() == "declaration"),
        "no declaration edit — the component itself was left as it was"
    );
    let direct = at(src, "f.source_path()") + "f.".len();
    assert!(edits.iter().any(|e| e.start == direct), "the direct accessor call was not renamed");
    assert!(edits.iter().all(|e| e.new_text == "sourcePath"));
}

/// The same chain with the conduit declared BY THE PROJECT — this always worked, and is kept as the
/// control: it isolates "the substitution machinery is sound" from "the library types resolve".
#[test]
fn a_project_declared_conduit_works_without_any_jdk() {
    let p = Project::new(&[
        ("p/Fn.java", "package p;\npublic interface Fn<A, B> { B apply(A a); }\n"),
        (
            "p/Seq.java",
            "package p;\npublic interface Seq<T> {\n    <R> Seq<R> map(Fn<? super T, ? extends R> f);\n}\n",
        ),
        (
            "p/Box.java",
            "package p;\npublic class Box {\n    public record Failure(String source_path) {}\n    Seq<Failure> failures;\n    void go() {\n        failures.map(failure -> failure.source_path());\n    }\n}\n",
        ),
    ]);
    let src = p.source("p/Box.java");
    let edits = p.rename_edits("p/Box.java", at(src, "source_path"), "sourcePath");
    let in_lambda = at(src, "failure.source_path()") + "failure.".len();
    assert!(edits.iter().any(|e| e.start == in_lambda));
}
