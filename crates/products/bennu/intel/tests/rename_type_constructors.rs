//! Renaming a type has to rename its CONSTRUCTORS. A constructor is spelled with the type's name
//! and nothing else — leave it behind and it stops being a constructor at all, which javac reports
//! as "invalid method declaration; return type required".
//!
//! Found by renaming every type in Apache Commons Lang: two classes broke this way, and both were
//! ordinary code (`IEEE754rUtils`, a nested `Iso8601_Rule`).

mod common;
use common::{at, Project};

const WITH_CTOR: &str = r#"package p;
public class Ieee754rUtils {
    private final int n;

    public Ieee754rUtils() {
        this(0);
    }

    public Ieee754rUtils(final int n) {
        this.n = n;
    }

    static class Iso8601_Rule {
        Iso8601_Rule(final int length) { }
    }
}
"#;

#[test]
fn a_type_rename_moves_its_constructors() {
    let p = Project::new(&[("p/Ieee754rUtils.java", WITH_CTOR)]);
    let src = p.source("p/Ieee754rUtils.java");
    let edits = p.rename_edits(
        "p/Ieee754rUtils.java",
        at(src, "Ieee754rUtils"),
        "IEEE754RUtils",
    );
    for needle in [
        "public Ieee754rUtils()",
        "public Ieee754rUtils(final int n)",
    ] {
        let site = at(src, needle) + "public ".len();
        assert!(
            edits.iter().any(|e| e.start == site),
            "the constructor at {needle:?} was not renamed; edits at {:?}",
            edits.iter().map(|e| e.start).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_nested_type_rename_moves_its_own_constructor_and_not_the_outers() {
    let p = Project::new(&[("p/Ieee754rUtils.java", WITH_CTOR)]);
    let src = p.source("p/Ieee754rUtils.java");
    let edits = p.rename_edits(
        "p/Ieee754rUtils.java",
        at(src, "class Iso8601_Rule") + "class ".len(),
        "Iso8601Rule",
    );
    let ctor = at(src, "Iso8601_Rule(final int length)");
    assert!(
        edits.iter().any(|e| e.start == ctor),
        "the nested type's constructor was not renamed; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
    // The OUTER class's constructors must be untouched.
    let outer = at(src, "public Ieee754rUtils()") + "public ".len();
    assert!(
        !edits.iter().any(|e| e.start == outer),
        "an unrelated constructor was renamed"
    );
}
