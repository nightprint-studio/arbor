//! Two types can each declare a nested type with the SAME simple name, and a member's declared type
//! is written with that simple name. Resolving it globally picks whichever landed in the project's
//! simple→binary map — so a parameter declared `Checker` on one interface was typed as the OTHER
//! interface's `Checker`, and everything downstream inherited the wrong signature.
//!
//! It surfaced as 27 false `argument-type` errors on a project the compiler builds clean, the
//! moment lambda parameters started being target-typed: a lambda's parameter type comes from the
//! functional interface, and the functional interface was the wrong one.

mod common;
use common::{at, Project};

const UPLOAD: &str = r#"package p;
public interface Upload {
    interface Checker {
        Boolean check(final String username, final Long idcom);
    }
    static Upload build(final Checker checker) { return null; }
}
"#;

const DOWNLOAD: &str = r#"package p;
public interface Download {
    interface Checker {
        String uuid(final String pin, final String identifier);
    }
    static Download build(final Checker checker) { return null; }
}
"#;

const SERVICE: &str = r#"package p;
public class Docs {
    public String byIdentifier(String identifier) { return identifier; }
}
"#;

const USER: &str = r#"package p;
public class Wiring4 {
    Download wire(final Docs service) {
        return Download.build((pin, identifier) -> service.byIdentifier(identifier));
    }
}
"#;

/// The lambda parameter must take its type from `Download.Checker` — the one its own interface
/// declares — not from the same-named nested type of an unrelated interface.
#[test]
fn a_nested_type_name_resolves_against_its_own_outer_type() {
    let p = Project::new(&[
        ("p/Upload.java", UPLOAD),
        ("p/Download.java", DOWNLOAD),
        ("p/Docs.java", SERVICE),
        ("p/Wiring4.java", USER),
    ]);
    // If `identifier` were typed from `Upload.Checker` it would be `Long`, and the call site would
    // not be recorded as a use of `byIdentifier(String)` at all.
    let decl = p.source("p/Docs.java");
    let edits = p.rename_edits("p/Docs.java", at(decl, "byIdentifier"), "byId");
    let user = p.source("p/Wiring4.java");
    let call = at(user, "service.byIdentifier(identifier)") + "service.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Wiring4.java" && e.start == call),
        "the call inside the lambda was not recorded; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}
