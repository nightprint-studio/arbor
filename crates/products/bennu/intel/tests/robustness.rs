//! Robustness / edge-case tests — the go-to-declaration entry point must NEVER panic, whatever
//! the caret lands on: whitespace, keywords, comments, string/number literals, operators, the
//! start/middle/end byte of an identifier, a syntactically broken file, an empty file, a
//! package-only file, and a unicode identifier. Where the rules guarantee a resolution we assert
//! it; where they don't, we assert the robust invariant (Some/None + correct file, never a crash).

mod common;
use common::*;

/// A small, well-formed single-file project used by most edge-case probes.
fn one() -> Project {
    Project::new(&[(
        "A.java",
        "package app;\n\
         // a leading line comment about the class\n\
         public class A {\n\
         \x20   private int count = 42;\n\
         \x20   public int bump(int delta) {\n\
         \x20       String label = \"hello world\";\n\
         \x20       int total = count + delta;\n\
         \x20       return total;\n\
         \x20   }\n\
         }\n",
    )])
}

#[test]
fn click_on_whitespace_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    // The blank spaces of the indentation before `return total;`.
    let off = at(&s, "       return total;");
    assert!(
        p.goto("A.java", off).is_none(),
        "whitespace resolves to nothing"
    );
}

#[test]
fn click_on_keyword_public_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    let off = at(&s, "public class A") + "pub".len();
    assert!(
        p.goto("A.java", off).is_none(),
        "the `public` keyword is not navigable"
    );
}

#[test]
fn click_on_keyword_return_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    let off = at(&s, "return total;") + "ret".len();
    assert!(
        p.goto("A.java", off).is_none(),
        "the `return` keyword is not navigable"
    );
}

#[test]
fn click_on_keyword_class_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    let off = at(&s, "class A {") + "cl".len();
    assert!(
        p.goto("A.java", off).is_none(),
        "the `class` keyword is not navigable"
    );
}

#[test]
fn click_inside_line_comment_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    // Land inside the word "comment" in the leading `//` line comment.
    let off = at(&s, "comment about") + "com".len();
    assert!(
        p.goto("A.java", off).is_none(),
        "a comment is not navigable"
    );
}

#[test]
fn click_inside_string_literal_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    // The word "world" lives inside the "hello world" string literal.
    let off = at(&s, "world\"");
    assert!(
        p.goto("A.java", off).is_none(),
        "text inside a string literal is not navigable"
    );
}

#[test]
fn click_on_number_literal_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    let off = at(&s, "42;");
    assert!(
        p.goto("A.java", off).is_none(),
        "a numeric literal is not navigable"
    );
}

#[test]
fn click_on_operator_is_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    // The `+` operator between `count` and `delta`.
    let off = at(&s, "count + delta") + "count ".len();
    assert!(
        p.goto("A.java", off).is_none(),
        "an operator token is not navigable"
    );
}

#[test]
fn identifier_start_middle_end_resolve_identically() {
    let p = one();
    let s = p.source("A.java").to_string();
    // The `count` USE inside `count + delta` — probe its first, a middle, and last byte.
    let base = at(&s, "count + delta");
    let start = base;
    let middle = base + 2; // the 'u'
    let end = base + "count".len() - 1; // the last 't'

    let d_start = p.goto("A.java", start).expect("goto at start byte");
    let d_middle = p.goto("A.java", middle).expect("goto at middle byte");
    let d_end = p.goto("A.java", end).expect("goto at end byte");

    // All three land on the same field declaration.
    assert_eq!(d_start.file, "A.java");
    assert_eq!(d_start.file, d_middle.file);
    assert_eq!(d_middle.file, d_end.file);
    assert_eq!(d_start.label, d_middle.label);
    assert_eq!(d_middle.label, d_end.label);
    assert_eq!(d_start.start, d_middle.start);
    assert_eq!(d_middle.start, d_end.start);
    assert_eq!(d_start.line, d_middle.line);
    assert_eq!(d_middle.line, d_end.line);
    // And it is the field, resolving in this same file.
    assert_eq!(d_start.label, "field app.A.count");
    assert_eq!(d_start.line, line_of(&s, "int count = 42"));
}

#[test]
fn local_variable_start_middle_end_resolve_identically() {
    let p = one();
    let s = p.source("A.java").to_string();
    // The `total` USE inside `return total;`.
    let base = at(&s, "return total;") + "return ".len();
    let start = base;
    let middle = base + 2;
    let end = base + "total".len() - 1;

    let d_start = p.goto("A.java", start).expect("goto local at start");
    let d_middle = p.goto("A.java", middle).expect("goto local at middle");
    let d_end = p.goto("A.java", end).expect("goto local at end");

    assert_eq!(d_start.file, "A.java");
    assert_eq!(d_start.start, d_middle.start);
    assert_eq!(d_middle.start, d_end.start);
    assert_eq!(d_start.label, d_middle.label);
    assert_eq!(d_middle.label, d_end.label);
    assert_eq!(d_start.label, "local `total`");
}

#[test]
fn broken_file_does_not_panic() {
    // Unbalanced braces, a dangling method with no body, a truncated statement.
    let p = Project::new(&[(
        "Broken.java",
        "package app;\n\
         public class Broken {\n\
         \x20   public int oops(int x) {\n\
         \x20       int y = x +\n\
         \x20   public void more(\n\
         }\n",
    )]);
    let s = p.source("Broken.java").to_string();
    // Probe several carets; each must return without panicking (Some or None both fine).
    let _ = p.goto("Broken.java", at(&s, "int x) {"));
    let _ = p.goto("Broken.java", at(&s, "int y = x"));
    let _ = p.goto("Broken.java", at(&s, "public void more"));
    let _ = p.goto("Broken.java", 0);
    let _ = p.goto("Broken.java", s.len().saturating_sub(1));
    // If we got here without unwinding, robustness holds.
    assert!(true);
}

#[test]
fn partial_expression_file_does_not_panic() {
    // A file that stops mid-expression, no closing brace at all.
    let p = Project::new(&[(
        "Partial.java",
        "package app;\n\
         class P {\n\
         \x20   int f() { return this.\n",
    )]);
    let s = p.source("Partial.java").to_string();
    let _ = p.goto("Partial.java", at(&s, "return this."));
    let _ = p.goto("Partial.java", at(&s, "int f()"));
    let _ = p.goto("Partial.java", s.len().saturating_sub(1));
    assert!(true, "a truncated expression must not crash go-to");
}

#[test]
fn empty_file_does_not_panic() {
    let p = Project::new(&[("Empty.java", "")]);
    // Offset 0 on a zero-length source: must be None, never a panic.
    assert!(
        p.goto("Empty.java", 0).is_none(),
        "empty file has nothing to resolve"
    );
}

#[test]
fn whitespace_only_file_does_not_panic() {
    let p = Project::new(&[("Blank.java", "   \n\t\n  \n")]);
    let s = p.source("Blank.java").to_string();
    assert!(p.goto("Blank.java", 0).is_none());
    let _ = p.goto("Blank.java", s.len().saturating_sub(1));
    assert!(true);
}

#[test]
fn package_only_file_does_not_panic() {
    let p = Project::new(&[("PkgOnly.java", "package app.only;\n")]);
    let s = p.source("PkgOnly.java").to_string();
    // Caret on the package name segment: no type/member decl to open → None, no panic.
    let off = at(&s, "app.only");
    assert!(p.goto("PkgOnly.java", off).is_none());
    // Caret on the `package` keyword.
    assert!(p.goto("PkgOnly.java", at(&s, "package")).is_none());
    assert!(true);
}

#[test]
fn unicode_identifier_does_not_panic() {
    // Non-ASCII identifiers (Java permits Unicode letters). Multi-byte chars stress the
    // byte-offset arithmetic in the resolver — it must not slice inside a char boundary.
    let p = Project::new(&[(
        "Uni.java",
        "package app;\n\
         public class Café {\n\
         \x20   private int número = 3;\n\
         \x20   public int léer() {\n\
         \x20       return número + 1;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Uni.java").to_string();
    // Caret on the `número` USE inside `return número + 1;` — must not panic.
    let use_off = at(&s, "return número") + "return ".len();
    let d = p.goto("Uni.java", use_off);
    // If it resolves, it must be to this file's field; if not, None is acceptable — never a crash.
    if let Some(loc) = d {
        assert_eq!(loc.file, "Uni.java");
    }
    // Caret on the unicode type name in its own declaration must also be safe.
    let _ = p.goto("Uni.java", at(&s, "class Café") + "class ".len());
    assert!(true, "unicode identifiers must not crash go-to");
}

#[test]
fn out_of_range_style_offsets_are_safe() {
    // Carets at exotic-but-valid byte positions: 0, the final byte, and just before EOF.
    let p = one();
    let s = p.source("A.java").to_string();
    let _ = p.goto("A.java", 0);
    let _ = p.goto("A.java", s.len().saturating_sub(1));
    // A caret at the closing brace of the class body.
    let _ = p.goto("A.java", at_last(&s, "}"));
    assert!(true);
}

#[test]
fn semicolon_and_braces_are_none() {
    let p = one();
    let s = p.source("A.java").to_string();
    // Operators / braces are not navigable — None, never a panic. (A caret ON a `;` right after
    // an identifier biases left onto that identifier by design, so probe an operator instead.)
    assert!(
        p.goto("A.java", at(&s, "count + delta") + "count ".len())
            .is_none(),
        "operator not navigable"
    );
    assert!(
        p.goto("A.java", at(&s, "class A {") + "class A ".len())
            .is_none(),
        "open brace not navigable"
    );
}

#[test]
fn find_usages_on_junk_caret_is_zero_not_panic() {
    // find-usages must be as robust as go-to: an unresolvable caret yields 0, not a panic.
    let p = one();
    let s = p.source("A.java").to_string();
    assert_eq!(
        p.usage_count("A.java", at(&s, "42;")),
        0,
        "a literal has no usages"
    );
    assert_eq!(
        p.usage_count("A.java", at(&s, "return total;") + "ret".len()),
        0,
        "the `return` keyword has no usages"
    );
}
