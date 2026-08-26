//! `Object @Nullable ... args` — a **type-use** annotation on the varargs marker (JSR-308).
//!
//! Legal Java that tree-sitter-java 0.23.5 cannot parse. Of eighteen type-use positions tested it
//! is the only failure, so `parse_java` masks it and re-parses rather than the workspace forking a
//! grammar over one construct.
//!
//! What the masking must NOT cost: any annotation Bennu actually consumes. Lombok, Spring and JPA
//! are all read from DECLARATION annotations — `@Value` on a parameter, `@Data` on a type — and
//! those sit before the type, not between it and the `...`.

use bennu_java::prelude::{extract_symbols, parse_java};

/// The construct itself, so a future grammar that fixes it makes this fail and the mask can go.
#[test]
fn the_grammar_still_cannot_parse_a_type_use_annotation_on_varargs() {
    const SRC: &str = "class A { static String g(Object @Nullable ... a) { return \"\"; } }";
    let mut p = tree_sitter::Parser::new();
    p.set_language(&bennu_java::prelude::java_language())
        .expect("language");
    let raw = p.parse(SRC, None).expect("parse");
    assert!(
        raw.root_node().has_error(),
        "the grammar now parses this — drop the mask in `bennu_java::grammar`"
    );
    // …and the crate's own entry point recovers it.
    let tree = parse_java(SRC).expect("parse");
    assert!(
        !tree.root_node().has_error(),
        "{}",
        tree.root_node().to_sexp()
    );
}

/// The user-facing worry, pinned: a DECLARATION annotation on the very same parameter list
/// survives, so anything keyed on one — a Spring `@Value` a plugin cross-checks against a YAML
/// property, Lombok, JPA — still finds it. Asserted on the PARSE, because that is where those
/// readers look: `ParamDecl` carries no annotations, so the CST is the model for them.
#[test]
fn a_declaration_annotation_on_the_same_parameter_survives_the_mask() {
    const SRC: &str = r#"package p;
public class Cfg {
    public void take(@Value("${app.name}") String label, Object @Nullable ... rest) { }
}
"#;
    let tree = parse_java(SRC).expect("parse");
    assert!(
        !tree.root_node().has_error(),
        "{}",
        tree.root_node().to_sexp()
    );
    let sexp = tree.root_node().to_sexp();
    assert!(
        sexp.contains("annotation"),
        "the `@Value` node is gone: {sexp}"
    );
    assert!(
        SRC[tree.root_node().byte_range()].contains("${app.name}"),
        "the annotation's argument must be readable from the ORIGINAL source"
    );

    // And the parameter the mask touched is a real parameter again, not a hole in the parse.
    let symbols = extract_symbols(SRC);
    let m = symbols.types[0]
        .methods
        .iter()
        .find(|m| m.name == "take")
        .expect("the method");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["label", "rest"], "{names:?}");
}

/// The mask is only ever reached by a parse that ALREADY failed, and only annotations sitting
/// against a `...` are blanked — so a file that parses is never touched.
#[test]
fn a_file_that_parses_is_never_masked() {
    const SRC: &str = "class A { @Deprecated void m(Object... a) { } }";
    let tree = parse_java(SRC).expect("parse");
    assert!(!tree.root_node().has_error());
    assert!(tree.root_node().to_sexp().contains("marker_annotation"));
}
