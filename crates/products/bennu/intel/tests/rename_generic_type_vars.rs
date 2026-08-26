//! A type variable is an identifier, not a letter. `record XmlParamEdit<Source, Param>` is legal
//! Java and common in this codebase — and the substitution that turns `edit.param()` into the
//! actual type argument only recognised names shaped like `T`, `E`, `R`, `T1`.
//!
//! The cost was not one call: `input.param()` typed as the literal `Param`, so every member read
//! off it was invisible, and renaming any of them left the call sites behind.

mod common;
use common::{at, Project};

const HOLDER: &str = r#"package p;
public record Edit<Source, Param>(Source source, Param param) { }
"#;

const PAYLOAD: &str = r#"package p;
public record Payload(String importo_offerto) { }
"#;

const USER: &str = r#"package p;
public class Parser {
    String parse(Edit<String, Payload> input) {
        return input.param().importo_offerto();
    }
}
"#;

#[test]
fn a_type_variable_with_a_long_name_is_substituted() {
    let p = Project::new(&[
        ("p/Edit.java", HOLDER),
        ("p/Payload.java", PAYLOAD),
        ("p/Parser.java", USER),
    ]);
    let decl = p.source("p/Payload.java");
    let edits = p.rename_edits(
        "p/Payload.java",
        at(decl, "String importo_offerto") + "String ".len(),
        "importoOfferto",
    );
    let user = p.source("p/Parser.java");
    let call = at(user, ".importo_offerto()") + ".".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Parser.java" && e.start == call),
        "the call through the generic accessor was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

/// The single-letter convention still has to work — it is what every JDK generic uses, and the
/// fallback for a library class whose declared list we could not decode.
const SHORT: &str = r#"package p;
public record Box<T>(T value) { }
"#;

const SHORT_USER: &str = r#"package p;
public class Unbox {
    String read(Box<Payload> b) {
        return b.value().importo_offerto();
    }
}
"#;

#[test]
fn a_single_letter_type_variable_still_works() {
    let p = Project::new(&[
        ("p/Box.java", SHORT),
        ("p/Payload.java", PAYLOAD),
        ("p/Unbox.java", SHORT_USER),
    ]);
    let decl = p.source("p/Payload.java");
    let edits = p.rename_edits(
        "p/Payload.java",
        at(decl, "String importo_offerto") + "String ".len(),
        "importoOfferto",
    );
    let user = p.source("p/Unbox.java");
    let call = at(user, ".importo_offerto()") + ".".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Unbox.java" && e.start == call),
        "edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

/// The shape from the project: a generic holder, whose type argument is a nested record, whose
/// component is read as a FIELD (legal from the enclosing class — a nested type's private members
/// are in scope there), and a call on that.
const NESTED_HOLDER: &str = r#"package p;
public class Parser2 {
    public static record Param(Payload filter) { }

    String parse(Edit<String, Param> input) {
        return input.param().filter.importo_offerto();
    }
}
"#;

#[test]
fn a_call_through_a_generic_then_a_record_component_field_is_renamed() {
    let p = Project::new(&[
        ("p/Edit.java", HOLDER),
        ("p/Payload.java", PAYLOAD),
        ("p/Parser2.java", NESTED_HOLDER),
    ]);
    let decl = p.source("p/Payload.java");
    let edits = p.rename_edits(
        "p/Payload.java",
        at(decl, "String importo_offerto") + "String ".len(),
        "importoOfferto",
    );
    let user = p.source("p/Parser2.java");
    let call = at(user, "filter.importo_offerto()") + "filter.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Parser2.java" && e.start == call),
        "the call through a record component read as a field was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

/// The type argument written as `Outer.Nested` — the spelling half this project uses. A dotted name
/// is only already-qualified when its head is NOT a type; treating it as qualified regardless built
/// a binary name with no package, and everything downstream of the generic resolved to nothing.
const QUALIFIED_ARG: &str = r#"package p;
public class Parser3 {
    public static record Param(Payload filter) { }

    String parse(Edit<String, Parser3.Param> input) {
        return input.param().filter.importo_offerto();
    }
}
"#;

#[test]
fn a_type_argument_written_as_outer_dot_nested_resolves() {
    let p = Project::new(&[
        ("p/Edit.java", HOLDER),
        ("p/Payload.java", PAYLOAD),
        ("p/Parser3.java", QUALIFIED_ARG),
    ]);
    let decl = p.source("p/Payload.java");
    let edits = p.rename_edits(
        "p/Payload.java",
        at(decl, "String importo_offerto") + "String ".len(),
        "importoOfferto",
    );
    let user = p.source("p/Parser3.java");
    let call = at(user, "filter.importo_offerto()") + "filter.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Parser3.java" && e.start == call),
        "the call through `Outer.Nested` as a type argument was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}
