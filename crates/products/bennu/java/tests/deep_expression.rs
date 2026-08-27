//! A deeply nested expression must not take the process down.
//!
//! Java puts no bound on how deep an expression may nest, and generated code reaches depths no
//! hand-written file ever would: a constant table built as `"a" + "a" + …`, a query assembled from
//! a few thousand fragments, a JSP compiled to one enormous `out.write(…)`. Each `+` is another
//! level of `binary_expression`, so the tree is as deep as the expression is long.
//!
//! Every walk that descends through expressions therefore has to be iterative. A recursive one
//! overflows the stack, and a stack overflow is not a caught error — it aborts the process. In
//! `bennu-be` that means the editor loses its backend over one file it merely opened.
//!
//! The depth here is the JDK's own: `test/langtools/tools/javac/DeepStringConcat.java` concatenates
//! 32001 literals, and is where this was found.

use bennu_java::prelude::{extract_symbols, parse_java};

/// The JDK's depth, in the JDK's shape.
fn deep_concat(terms: usize) -> String {
    let concat = vec!["\"a\""; terms].join("+");
    format!("public class D {{ public static final String X = {concat}; }}")
}

#[test]
fn a_thirty_thousand_term_concatenation_is_parsed_and_walked() {
    let src = deep_concat(32001);
    assert!(parse_java(&src).is_some(), "the grammar handles the depth; the walks must too");

    let symbols = extract_symbols(&src);
    assert_eq!(symbols.types.len(), 1, "the class is still found past the deep initializer");
    assert_eq!(symbols.types[0].name, "D");
}

/// The same depth inside an annotation element, which a separate walk reads.
#[test]
fn a_deep_concatenation_in_an_annotation_is_walked() {
    let concat = vec!["\"a\""; 20000].join("+");
    let src = format!(
        "public class D {{ @SuppressWarnings({concat}) void m() {{}} }}"
    );
    let symbols = extract_symbols(&src);
    assert_eq!(symbols.types.len(), 1);
}

/// Depth reached through nested anonymous classes, whose walk numbers them in tree order — the
/// walk that had to keep its ordering while losing its recursion.
#[test]
fn deeply_nested_expressions_do_not_disturb_anonymous_class_numbering() {
    let filler = vec!["\"a\""; 5000].join("+");
    let src = format!(
        r#"
public class D {{
    String pad = {filler};
    Runnable a = new Runnable() {{ public void run() {{}} }};
    Runnable b = new Runnable() {{ public void run() {{}} }};
}}
"#
    );
    let symbols = extract_symbols(&src);
    let names: Vec<&str> = symbols.types.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"D"), "got {names:?}");
    // Two anonymous bodies, numbered in the order they are written.
    assert!(names.contains(&"1") && names.contains(&"2"), "got {names:?}");
}
