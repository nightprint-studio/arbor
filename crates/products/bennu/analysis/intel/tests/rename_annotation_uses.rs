//! `@Ann` is a use of the type `Ann`. The walk indexed `type_identifier` nodes and an annotation's
//! name is not one — it is the `name` field of a `marker_annotation` / `annotation` — so every
//! annotation use was invisible to the index.
//!
//! Renaming the annotation then rewrote its declaration and its IMPORTS (found by a source walk)
//! and left every `@Ann` behind. On Guava that was 196 files: the import said `J2KtIncompatible`
//! and the annotation still said `@J2ktIncompatible`.

mod common;
use common::{at, Project};

const ANN: &str = r#"package p;
public @interface J2ktIncompatible { }
"#;

const MARKER_USER: &str = r#"package p;
@J2ktIncompatible
public class Defaults {
    @J2ktIncompatible
    void m() { }
}
"#;

const ARGS_USER: &str = r#"package p;
public class Other {
    @J2ktIncompatible()
    void m() { }
}
"#;

fn project() -> Project {
    Project::new(&[
        ("p/J2ktIncompatible.java", ANN),
        ("p/Defaults.java", MARKER_USER),
        ("p/Other.java", ARGS_USER),
    ])
}

#[test]
fn a_marker_annotation_use_is_renamed() {
    let p = project();
    let decl = p.source("p/J2ktIncompatible.java");
    let edits = p.rename_edits(
        "p/J2ktIncompatible.java",
        at(decl, "J2ktIncompatible"),
        "J2KtIncompatible",
    );
    let user = p.source("p/Defaults.java");
    for occurrence in 0..2 {
        let mut i = 0usize;
        for _ in 0..=occurrence {
            i = user[i..].find("@J2ktIncompatible").unwrap()
                + i
                + if occurrence == 0 { 0 } else { 1 };
        }
        let site = user
            .match_indices("@J2ktIncompatible")
            .nth(occurrence)
            .unwrap()
            .0
            + 1;
        assert!(
            edits
                .iter()
                .any(|e| e.file == "p/Defaults.java" && e.start == site),
            "annotation use #{occurrence} was not renamed; edits: {:?}",
            edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn an_annotation_with_an_argument_list_is_renamed_too() {
    let p = project();
    let decl = p.source("p/J2ktIncompatible.java");
    let edits = p.rename_edits(
        "p/J2ktIncompatible.java",
        at(decl, "J2ktIncompatible"),
        "J2KtIncompatible",
    );
    let user = p.source("p/Other.java");
    let site = at(user, "@J2ktIncompatible()") + 1;
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Other.java" && e.start == site),
        "edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}
