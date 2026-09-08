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

/// **A type's own nested class must not capture a name written in its HEADER.**
///
/// `class HashCodeBuilder … implements Builder<Integer>`, in a class that also declares a nested
/// `Builder`, names the same-package INTERFACE: a member type's scope is the *body* of its class
/// (JLS §6.3), and `extends`/`implements` are not the body. That is why javac compiles
/// commons-lang, where this shape is written four times.
///
/// Read in its own scope, the header bound to the nested class — so the interface's `build()` was
/// in no supertype of it, the `@Override` was reported as overriding nothing, and renaming the
/// method moved neither the interface's own declaration nor the other implementors.
#[test]
fn a_headers_supertype_is_not_the_types_own_nested_class() {
    let p = Project::new(&[
        (
            "Builder.java",
            "package b;\n\
             public interface Builder<T> {\n\
             \x20   T build();\n\
             }\n",
        ),
        (
            "HashCodeBuilder.java",
            "package b;\n\
             public class HashCodeBuilder implements Builder<Integer> {\n\
             \x20   public static class Builder {\n\
             \x20       public HashCodeBuilder get() { return null; }\n\
             \x20   }\n\
             @Override\n\
             \x20   public Integer build() { return Integer.valueOf(1); }\n\
             }\n",
        ),
    ]);
    // The header named the interface, so the class implements it and the `@Override` is one.
    assert_eq!(p.validate_errors("HashCodeBuilder.java"), Vec::<String>::new());

    // And the rename carries the interface's declaration with it — the half a wrong supertype
    // silently dropped.
    let src = p.source("HashCodeBuilder.java").to_string();
    let at_build = at(&src, "public Integer build()") + "public Integer ".len();
    let plan = p.rename("HashCodeBuilder.java", at_build, "Build").expect("a plan");
    let files: Vec<String> = plan
        .files
        .iter()
        .map(|f| f.file.rsplit(['/', '\\']).next().unwrap_or(&f.file).to_string())
        .collect();
    assert!(
        files.iter().any(|f| f == "Builder.java"),
        "the interface's own `build()` must be renamed too, got {files:?}",
    );
}
