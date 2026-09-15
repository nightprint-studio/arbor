//! An `enum` constant with a body is an anonymous subclass of its own enum, and its methods override
//! the enum's abstract ones. Renaming one of those methods has to move the whole family, from
//! whichever end it is started.
//!
//! The direction that failed is the one people actually use: starting from a constant's body.
//! Renaming from the enum's abstract declaration worked, so the feature looked present — measured on
//! Commons Lang's `StopWatch.State`, where a rename begun in `RUNNING { … }` moved that one method
//! and left the enum, the abstract declaration and the three sibling constants behind. The constant
//! then declared a method overriding nothing, and javac rejected the file the rename had just
//! "successfully" applied.
//!
//! The cause was a level below the rename: the extractor recorded a constant with a body as a FIELD
//! of the enum and stopped there, so the body's own type — the thing that declares those overrides —
//! did not exist at all.

mod common;
use common::{at, Project};

/// The abstract declaration and every constant body move together, started from a BODY.
#[test]
fn renaming_from_a_constant_body_moves_the_whole_enum_family() {
    let p = Project::new(&[(
        "p/State.java",
        "package p;\npublic enum State {\n    RUNNING {\n        @Override boolean is_started() { return true; }\n    },\n    STOPPED {\n        @Override boolean is_started() { return false; }\n    };\n    abstract boolean is_started();\n}\n",
    )]);
    let src = p.source("p/State.java");
    // The caret on the FIRST occurrence — the override inside `RUNNING`'s body.
    let edits = p.rename_edits("p/State.java", at(src, "is_started"), "isStarted");
    let decls = edits
        .iter()
        .filter(|e| e.reason.label() == "declaration")
        .count();
    assert_eq!(
        decls, 3,
        "expected the two constant bodies and the abstract declaration; got {decls}: {:?}",
        edits.iter().map(|e| (e.start, e.reason.label())).collect::<Vec<_>>()
    );
}

/// The same family, started from the abstract declaration — the direction that already worked, kept
/// as the other half of the invariant.
#[test]
fn renaming_from_the_abstract_declaration_moves_the_constant_bodies() {
    let p = Project::new(&[(
        "p/State.java",
        "package p;\npublic enum State {\n    RUNNING {\n        @Override boolean is_started() { return true; }\n    };\n    abstract boolean is_started();\n}\n",
    )]);
    let src = p.source("p/State.java");
    let offset = src.rfind("is_started").expect("the abstract declaration");
    let edits = p.rename_edits("p/State.java", offset, "isStarted");
    let decls = edits
        .iter()
        .filter(|e| e.reason.label() == "declaration")
        .count();
    assert_eq!(decls, 2, "{:?}", edits.iter().map(|e| e.start).collect::<Vec<_>>());
}
