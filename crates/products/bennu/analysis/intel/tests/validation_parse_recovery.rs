//! Validation across a **parse error** — what a half-typed member costs the rest of the file.
//!
//! Until this, a file whose tree carried an `ERROR` node got its syntax error and nothing else.
//! The reason was real: a tree nobody believes produces diagnostics about code that compiles. What
//! was wrong was the size of the thing given up. A file is edited one member at a time, and for
//! most of the time it spends being edited exactly one member does not parse — which is when the
//! checks are worth the most, and when every one of them went quiet.
//!
//! Half of this file is the containment, because that is where the change goes wrong: the broken
//! member itself must still say nothing but "this does not parse".

mod common;
use common::*;

fn project(use_src: &str) -> Project {
    Project::new(&[
        (
            "Cfg.java",
            "package shop;\npublic class Cfg {\n    public String name() { return \"x\"; }\n}\n",
        ),
        ("Use.java", use_src),
    ])
}

// ── What a broken member no longer costs ─────────────────────────────────────────────────────

#[test]
fn a_broken_method_does_not_silence_the_next_one() {
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void half() { int x = = ; }\n\
         \x20   void whole() { Cfg c = new Cfg(); c.nome(); }\n\
         }\n",
    );
    let errors = p.validate_errors("Use.java");
    assert!(
        errors.iter().any(|e| e.starts_with("unknown-member")),
        "the method that DID parse must still be checked: {errors:?}"
    );
}

#[test]
fn the_broken_member_still_reports_its_syntax_error() {
    // The one diagnostic that lives inside the quarantined member by construction. Dropping it
    // would answer a file that does not parse with silence.
    let p = project("package shop;\npublic class Use {\n    void half() { int x = = ; }\n}\n");
    let errors = p.validate_errors("Use.java");
    assert!(
        errors.iter().any(|e| e.starts_with("syntax") || e.starts_with("missing-token")),
        "expected the syntax error itself: {errors:?}"
    );
}

#[test]
fn a_broken_method_does_not_silence_a_nested_class() {
    // The reported case: a nested type's own members were checked by nobody while anything above
    // them was mid-edit.
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void half() { int x = = ; }\n\
         \x20   public static class Inner {\n\
         \x20       void go() { Cfg c = new Cfg(); c.nome(); }\n\
         \x20   }\n\
         }\n",
    );
    let errors = p.validate_errors("Use.java");
    assert!(
        errors.iter().any(|e| e.starts_with("unknown-member")),
        "the nested class must still be checked: {errors:?}"
    );
}

// ── What it still costs ──────────────────────────────────────────────────────────────────────

#[test]
fn nothing_but_the_syntax_error_is_reported_inside_the_broken_member() {
    // The whole reason the file-wide gate existed: recovery reads whatever it can as code. Every
    // diagnostic anywhere in that member is a claim about a nesting nobody wrote.
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void half() { Cfg c = new Cfg(); c.nome(; int x = = ; }\n\
         \x20   void whole() { }\n\
         }\n",
    );
    let errors = p.validate_errors("Use.java");
    let leaked: Vec<&String> = errors
        .iter()
        .filter(|e| !e.starts_with("syntax") && !e.starts_with("missing-token"))
        .collect();
    assert!(leaked.is_empty(), "diagnostics leaked out of the broken member: {leaked:?}");
}

#[test]
fn an_unbalanced_class_brace_still_gives_up_the_whole_file() {
    // Nothing smaller than the file is to blame: the brace changes what every member below it is
    // nested in, so there is no member to charge the error to.
    let p = project(
        "package shop;\n\
         public class Use {\n\
         \x20   void whole() { Cfg c = new Cfg(); c.nome(); }\n",
    );
    let errors = p.validate_errors("Use.java");
    let leaked: Vec<&String> = errors
        .iter()
        .filter(|e| !e.starts_with("syntax") && !e.starts_with("missing-token"))
        .collect();
    assert!(leaked.is_empty(), "expected the file-wide gate to hold: {leaked:?}");
}

#[test]
fn a_string_literal_that_recovery_ends_early_reports_nothing_but_syntax() {
    // The measured regression this gate was built for: a fuzzer file whose payload is a huge
    // string literal, whose CONTENTS were read as code and reported as 199 undefined symbols.
    let payload = "public class Nope { void boom() { int Ë = t; } } ".repeat(40);
    let src = format!(
        "package shop;\npublic class Use {{\n    String s = \"{payload}\n    void whole() {{ }}\n}}\n"
    );
    let p = project(&src);
    let errors = p.validate_errors("Use.java");
    let leaked: Vec<&String> = errors
        .iter()
        .filter(|e| !e.starts_with("syntax") && !e.starts_with("missing-token"))
        .collect();
    assert!(leaked.len() < 5, "the literal's contents leaked back in ({}): {leaked:?}", leaked.len());
}
