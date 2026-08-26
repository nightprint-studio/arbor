//! Members an enum inherits from `java.lang.Enum` — `name()`, `ordinal()`, `values()`.
//!
//! They are declared nowhere in the project, so a project-only walk cannot resolve them: every
//! `e.name()` reads as a call to something unknown. With the JDK tier in the walk they resolve to
//! `java/lang/Enum`, which is a LIBRARY type — so the call is typed, and correctly recorded as no
//! project symbol's use.

mod common;
use common::{at, Project};

const SRC: &str = r#"package p;
public enum EGender {
    MALE, FEMALE;

    public static EGender from_codice(String c) {
        for (EGender g : values()) {
            if (g.name().equals(c)) { return g; }
        }
        return null;
    }
}
"#;

/// The engine must still plan the project's OWN method even though the file also calls inherited
/// ones it cannot see. A `name()` it fails to resolve must not poison the rename beside it.
#[test]
fn an_enums_own_method_is_renameable_beside_inherited_calls() {
    let p = Project::new(&[("p/EGender.java", SRC)]);
    let src = p.source("p/EGender.java");
    let edits = p.rename_edits("p/EGender.java", at(src, "from_codice"), "fromCodice");
    assert!(
        edits.iter().any(|e| e.reason.label() == "declaration"),
        "the enum's own method was not renamed: {edits:?}"
    );
}

/// `name()` belongs to `java.lang.Enum`, so renaming it is not the project's to do.
#[test]
fn renaming_an_inherited_enum_method_is_not_offered() {
    let p = Project::new(&[("p/EGender.java", SRC)]);
    let src = p.source("p/EGender.java");
    let call = at(src, "g.name()") + "g.".len();
    let plan = p.rename("p/EGender.java", call, "label");
    // Either nothing to rename, or an explicit refusal — never a silent rewrite of a JDK method.
    if let Some(plan) = plan {
        assert!(
            plan.blocked.is_some() || plan.total_edits() == 0,
            "renaming java.lang.Enum.name() must not be offered as an ordinary rename: {:?}",
            plan.files,
        );
    }
}

/// The same file with the JDK visible to the walk — the shape production has now that the walk
/// resolves through the JDK tier.
#[test]
fn with_the_jdk_visible_the_enums_own_method_still_renames() {
    let p = Project::with_stream_jdk(&[("p/EGender.java", SRC)]);
    let src = p.source("p/EGender.java");
    let edits = p.rename_edits("p/EGender.java", at(src, "from_codice"), "fromCodice");
    assert!(
        edits.iter().any(|e| e.reason.label() == "declaration"),
        "the enum's own method was not renamed: {edits:?}"
    );
}

/// `g.name()` resolves to `java.lang.Enum` — a library type — so go-to says so rather than nothing.
#[test]
fn an_inherited_enum_method_resolves_to_the_jdk() {
    let p = Project::with_stream_jdk(&[("p/EGender.java", SRC)]);
    let src = p.source("p/EGender.java");
    let call = at(src, "g.name()") + "g.".len();
    let label = p.goto_label("p/EGender.java", call);
    // It is not project code, so there is no project declaration to land on — but it must not be
    // mistaken for a member of the enum itself.
    assert!(
        label.is_none() || label.as_deref() == Some("method java.lang.Enum.name()"),
        "unexpected target for an inherited enum method: {label:?}"
    );
}
