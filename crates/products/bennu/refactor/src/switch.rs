//! **Replace an `if` chain with a `switch`** — one subject, constant labels, one statement.
//!
//! ```java
//! if (kind == 1) { a(); }              switch (kind) {
//! else if (kind == 2) { b(); }    →        case 1: { a(); break; }
//! else { c(); }                            case 2: { b(); break; }
//!                                          default: { c(); break; }
//!                                      }
//! ```
//!
//! ## The shape it insists on, and why every part of it is load-bearing
//!
//! A chain converts only when **every rung tests the same subject against a compile-time constant**.
//! That sounds like a formality and is the whole correctness argument:
//!
//! - **The same subject**, because a `switch` evaluates it once. A chain testing `a` then `b` has no
//!   subject to put in the parentheses.
//! - **Evaluated once**, so the subject has to be something re-reading cannot change — a name, a
//!   field, `this.x`. A **call** is refused: the chain ran it per rung, the `switch` runs it once,
//!   and whether that matters is a question about a method this crate cannot see.
//! - **A constant label**, because `case` takes nothing else. Literals qualify; so does an enum
//!   constant, which is why `Kind.BIG` is recognised and written back as bare `BIG` — inside a
//!   `switch` over an enum the qualified form does not compile.
//! - **`s.equals("a")`, never `"a".equals(s)`.** The flipped form is the null-safe idiom: on a null
//!   `s` it is false and the chain falls through, where `switch (s)` throws. Converting it would
//!   turn a working program into one that throws, which is the one thing a refactoring may not do.
//!
//! ## `break`, and why an extra one is a compile error rather than a wart
//!
//! Each arm needs a `break` unless its body already leaves — and a `break` written after a body that
//! cannot complete normally is an **unreachable statement**, which javac rejects (JLS §14.21). So
//! the body's completion has to be *known*, not guessed: shapes this can decide get their answer,
//! and a body ending in a `try`, a nested `switch` or a labelled statement makes the whole
//! conversion a refusal. Guessing here produces either a file that does not compile or a silent
//! fall-through, and there is no third outcome worth having.
//!
//! ## Colons, not arrows
//!
//! `case L: { … }` and not `case L -> { … }`: the arrow form is Java 14, and this editor exists for
//! codebases that are not. The braces are not decoration either — the arms of a colon `switch` share
//! one scope, so two arms each declaring `int n` collide without them.

use tree_sitter::Node;

use crate::body::reindent;
use crate::plan::{Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{enclosing, indent_at, is_block, newline, text};

const ID: (&str, &str) = ("if-chain-to-switch", "Replace `if` chain with `switch`");

/// The fewest branches worth a `switch`. Two is an `if`/`else`, and saying so in the menu on every
/// two-branch `if` in a file is noise rather than an offer.
const ENOUGH_BRANCHES: usize = 3;

/// Plan a *replace `if` chain with `switch`* for the chain the caret's `if` heads.
pub fn if_chain_to_switch(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = ID;
    let head = chain_head(root, start, end)?;

    let mut rungs: Vec<Rung<'_>> = Vec::new();
    let mut default: Option<Node<'_>> = None;
    let mut current = head;
    loop {
        let condition = current.child_by_field_name("condition")?;
        let consequence = current.child_by_field_name("consequence")?;
        let Some(test) = read_test(&condition, source) else {
            // Silence, not a refusal: an `if` chain that was never about one value is not a
            // gesture towards a `switch`, and a greyed row on every `if` in a file is noise.
            return None;
        };
        rungs.push(Rung { test, body: consequence });
        match current.child_by_field_name("alternative") {
            Some(next) if next.kind() == "if_statement" => current = next,
            Some(last) => {
                default = Some(last);
                break;
            }
            None => break,
        }
    }
    if rungs.len() + usize::from(default.is_some()) < ENOUGH_BRANCHES {
        return None;
    }

    // One subject, spelled one way. Two spellings of the same thing (`x` and `this.x`) are refused
    // rather than unified: which one the `switch` should say is a question about the code, not about
    // the refactoring.
    let subject = rungs[0].test.subject.to_string();
    if let Some(other) = rungs.iter().find(|r| r.test.subject != subject) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!(
                "the rungs test two different things — `{subject}` and `{}` — and a `switch` has one subject",
                other.test.subject
            ),
        )));
    }
    let kind = rungs[0].test.kind;
    if rungs.iter().any(|r| r.test.kind != kind) {
        return Some(Err(Refusal::new(
            id,
            label,
            "some rungs compare with `==` and others with `equals`, which are not the same test",
        )));
    }
    // Two arms with one label is a `switch` that does not compile — and in the chain the second one
    // was already dead code, which is worth being told about.
    for (i, rung) in rungs.iter().enumerate() {
        if rungs[..i].iter().any(|earlier| earlier.test.label == rung.test.label) {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("two rungs test `{}`, and a `switch` may only label an arm once", rung.test.label),
            )));
        }
    }

    let base = indent_at(source, head.start_byte());
    let unit = indent_unit(source, &rungs[0].body, &base);
    let nl = newline(source);
    let mut out = format!("switch ({subject}) {{{nl}");
    for rung in &rungs {
        let arm = render_arm(&format!("case {}", rung.test.label), &rung.body, source, &base, &unit, nl);
        match arm {
            Some(text) => out.push_str(&text),
            None => return Some(Err(unknown_completion(id, label))),
        }
    }
    if let Some(default) = default {
        match render_arm("default", &default, source, &base, &unit, nl) {
            Some(text) => out.push_str(&text),
            None => return Some(Err(unknown_completion(id, label))),
        }
    }
    out.push_str(&format!("{base}}}"));

    let edits = vec![RefactorEdit::new(head.start_byte(), head.end_byte(), out, "switch")];
    Some(Ok(Plan::new(id, label, edits).caret_at(head.start_byte())))
}

/// One rung of the chain: what it tests, and what it does.
struct Rung<'t> {
    test: Test,
    body: Node<'t>,
}

/// A rung's test, read into the two halves a `switch` needs.
#[derive(PartialEq, Eq)]
struct Test {
    /// The expression the `switch` will stand on, as the source writes it.
    subject: String,
    /// The `case` label, written the way a `switch` needs it — an enum constant loses its type.
    label: String,
    kind: TestKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TestKind {
    /// `x == 1`, `kind == Kind.BIG`
    Equality,
    /// `s.equals("a")`
    Equals,
}

fn unknown_completion(id: &str, label: &str) -> Refusal {
    Refusal::new(
        id,
        label,
        "one arm ends in a `try`, a `switch` or a labelled statement, and whether a `break` after \
         it would be reachable cannot be read off the text — javac rejects an unreachable one",
    )
}

/// The `if` a caret is standing on, and only through its own head — the same rule
/// [`crate::if_statement`] uses, and for the same reason: standing inside the guarded code is not
/// standing on the guard.
///
/// The chain is then taken from **this** `if` down. A caret on a middle rung converts the rest of
/// the ladder, which is what "replace this chain" means from where the user is standing.
fn chain_head<'t>(root: Node<'t>, start: usize, end: usize) -> Option<Node<'t>> {
    let at = crate::selection::node_covering(root, start, end)?;
    let stmt = enclosing(at, &["if_statement"])?;
    let condition = stmt.child_by_field_name("condition")?;
    (start >= stmt.start_byte() && start <= condition.end_byte()).then_some(stmt)
}

/// Read a rung's condition into a subject and a `case` label, or `None` when it is not that shape.
fn read_test(condition: &Node<'_>, source: &str) -> Option<Test> {
    let expr = strip_parens(condition);
    match expr.kind() {
        "binary_expression" => {
            let operator = expr.child_by_field_name("operator")?;
            if text(&operator, source) != "==" {
                return None;
            }
            let left = expr.child_by_field_name("left")?;
            let right = expr.child_by_field_name("right")?;
            // Either side may hold the constant; the other is then the subject.
            if let Some(label) = case_label(&right, source) {
                return is_a_subject(&left, source)
                    .map(|subject| Test { subject, label, kind: TestKind::Equality });
            }
            let label = case_label(&left, source)?;
            is_a_subject(&right, source).map(|subject| Test { subject, label, kind: TestKind::Equality })
        }
        // `s.equals("a")` — and deliberately not `"a".equals(s)`; see the module docs.
        "method_invocation" => {
            let name = expr.child_by_field_name("name")?;
            if text(&name, source) != "equals" {
                return None;
            }
            let arguments = expr.child_by_field_name("arguments")?;
            let mut cursor = arguments.walk();
            let args: Vec<Node<'_>> = arguments.named_children(&mut cursor).collect();
            let [only] = args.as_slice() else { return None };
            if only.kind() != "string_literal" {
                return None;
            }
            let receiver = expr.child_by_field_name("object")?;
            is_a_subject(&receiver, source).map(|subject| Test {
                subject,
                label: text(only, source).to_string(),
                kind: TestKind::Equals,
            })
        }
        _ => None,
    }
}

/// The expression a `switch` may stand on: something re-reading cannot change.
///
/// A call is not one — see the module docs — and neither is anything with an assignment or an
/// increment in it.
fn is_a_subject(node: &Node<'_>, source: &str) -> Option<String> {
    let node = strip_parens(node);
    match node.kind() {
        "identifier" => Some(text(&node, source).to_string()),
        "field_access" => {
            // `this.kind` and `outer.kind` are fine; `f().kind` is not.
            let object = node.child_by_field_name("object")?;
            matches!(object.kind(), "this" | "identifier" | "field_access")
                .then(|| text(&node, source).to_string())
        }
        _ => None,
    }
}

/// The `case` label an expression can be, written the way a `switch` needs it.
fn case_label(node: &Node<'_>, source: &str) -> Option<String> {
    let node = strip_parens(node);
    match node.kind() {
        "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal"
        | "character_literal"
        | "string_literal" => Some(text(&node, source).to_string()),
        // An enum constant, which inside its own `switch` must be written bare: `case BIG`, never
        // `case Kind.BIG`. Recognised by shape — a constant read off a type — because the
        // alternative is a resolver this crate does not have.
        "field_access" => {
            let object = node.child_by_field_name("object")?;
            let field = node.child_by_field_name("field")?;
            let type_like = matches!(object.kind(), "identifier" | "scoped_identifier")
                && text(&object, source).rsplit('.').next().is_some_and(starts_upper);
            let constant_like = is_screaming(text(&field, source));
            (type_like && constant_like).then(|| text(&field, source).to_string())
        }
        _ => None,
    }
}

fn starts_upper(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// `BIG`, `TOO_BIG` — an enum constant as everybody writes one. A convention, and the only reason
/// leaning on it is safe: getting it wrong produces `case notAConstant`, which javac rejects at once
/// rather than accepting as something else.
fn is_screaming(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && name.chars().any(|c| c.is_ascii_uppercase())
}

/// One arm of the produced `switch`, or `None` when its body's completion cannot be read.
fn render_arm(
    label: &str,
    body: &Node<'_>,
    source: &str,
    base: &str,
    unit: &str,
    nl: &str,
) -> Option<String> {
    let leaves = !completes_normally(body, source)?;
    let arm_indent = format!("{base}{unit}");
    let inner_indent = format!("{arm_indent}{unit}");
    let mut out = format!("{arm_indent}{label}: {{{nl}");
    for line in inner_lines(body, source) {
        out.push_str(&reindent(&line.0, &line.1, &inner_indent));
        out.push_str(nl);
    }
    if !leaves {
        out.push_str(&format!("{inner_indent}break;{nl}"));
    }
    out.push_str(&format!("{arm_indent}}}{nl}"));
    Some(out)
}

/// The statements of an arm's body, each with the indentation it currently carries — so
/// [`reindent`] can move it without flattening what is nested inside it.
fn inner_lines(body: &Node<'_>, source: &str) -> Vec<(String, String)> {
    let statements: Vec<Node<'_>> = if is_block(body) {
        let mut cursor = body.walk();
        body.named_children(&mut cursor).collect()
    } else {
        vec![*body]
    };
    statements
        .into_iter()
        .map(|s| {
            let indent = indent_at(source, s.start_byte());
            (format!("{indent}{}", text(&s, source)), indent)
        })
        .collect()
}

/// The indentation step this file is written with, read off the arm's own body rather than assumed.
fn indent_unit(source: &str, body: &Node<'_>, base: &str) -> String {
    let inner = if is_block(body) {
        body.named_child(0).map(|s| indent_at(source, s.start_byte())).unwrap_or_default()
    } else {
        indent_at(source, body.start_byte())
    };
    match inner.strip_prefix(base) {
        Some(step) if !step.is_empty() => step.to_string(),
        _ => "    ".to_string(),
    }
}

/// Whether a statement can complete normally — JLS §14.21, for the shapes that can be decided.
///
/// `None` is "cannot say", and it is a refusal rather than a default because both defaults are
/// wrong: guessing *normally* writes a `break` javac calls unreachable, and guessing *abruptly*
/// writes an arm that falls through into the next one.
fn completes_normally(node: &Node<'_>, source: &str) -> Option<bool> {
    match node.kind() {
        "return_statement" | "throw_statement" | "yield_statement" => Some(false),
        // Inside the `switch` being written, a bare `break` would bind to it rather than to what it
        // binds to now. A labelled one still leaves.
        "break_statement" | "continue_statement" => Some(false),
        "expression_statement"
        | "local_variable_declaration"
        | "assert_statement"
        | "line_comment"
        | "block_comment"
        | ";" => Some(true),
        "block" | "constructor_body" => {
            let mut cursor = node.walk();
            let statements: Vec<Node<'_>> = node
                .named_children(&mut cursor)
                .filter(|c| !matches!(c.kind(), "line_comment" | "block_comment"))
                .collect();
            match statements.last() {
                Some(last) => completes_normally(last, source),
                None => Some(true),
            }
        }
        "if_statement" => {
            let consequence = node.child_by_field_name("consequence")?;
            match node.child_by_field_name("alternative") {
                // An `if` with no `else` always can: the test may be false.
                None => Some(true),
                Some(alternative) => Some(
                    completes_normally(&consequence, source)?
                        || completes_normally(&alternative, source)?,
                ),
            }
        }
        // A loop can complete normally unless it is the endless kind — and `while (true)` with a
        // `break` inside can, which is why only the shape with no `break` at all is decided here.
        "while_statement" => {
            let condition = node.child_by_field_name("condition")?;
            let endless = strip_parens(&condition).kind() == "true";
            if endless && crate::selection::descendants(*node, "break_statement").is_empty() {
                Some(false)
            } else {
                Some(true)
            }
        }
        "for_statement" | "enhanced_for_statement" | "do_statement" => Some(true),
        _ => None,
    }
}

/// Walk down through `(…)` to the expression they wrap.
fn strip_parens<'t>(node: &Node<'t>) -> Node<'t> {
    let mut current = *node;
    while matches!(current.kind(), "parenthesized_expression" | "condition") {
        match current.named_child(0) {
            Some(inner) => current = inner,
            None => break,
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn outcome(source: &str, needle: &str) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        if_chain_to_switch(tree.root_node(), source, at, at)
    }

    fn applied(source: &str, needle: &str) -> String {
        match outcome(source, needle) {
            Some(Ok(plan)) => plan.apply(source),
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    fn refusal(source: &str, needle: &str) -> String {
        match outcome(source, needle) {
            Some(Err(r)) => r.reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_chain_on_one_int_becomes_a_switch() {
        let src = "class A {\n    void f(int kind) {\n        if (kind == 1) {\n            a();\n        } else if (kind == 2) {\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        let out = applied(src, "if (kind == 1)");
        assert!(out.contains("switch (kind) {"), "{out}");
        assert!(out.contains("case 1: {"), "{out}");
        assert!(out.contains("case 2: {"), "{out}");
        assert!(out.contains("default: {"), "{out}");
        assert_eq!(out.matches("break;").count(), 3, "{out}");
    }

    /// An arm that already leaves must not gain a `break` — javac calls it unreachable.
    #[test]
    fn an_arm_that_returns_gets_no_break() {
        let src = "class A {\n    int f(int kind) {\n        if (kind == 1) {\n            return 1;\n        } else if (kind == 2) {\n            return 2;\n        } else {\n            return 0;\n        }\n    }\n}";
        let out = applied(src, "if (kind == 1)");
        assert!(!out.contains("break;"), "{out}");
    }

    /// An enum constant is written bare inside its own `switch`.
    #[test]
    fn an_enum_constant_loses_its_type() {
        let src = "class A {\n    void f(Kind kind) {\n        if (kind == Kind.BIG) {\n            a();\n        } else if (kind == Kind.SMALL) {\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        let out = applied(src, "if (kind ==");
        assert!(out.contains("case BIG: {"), "{out}");
        assert!(!out.contains("Kind.BIG"), "{out}");
    }

    #[test]
    fn a_string_chain_written_the_safe_way_round_converts() {
        let src = "class A {\n    void f(String s) {\n        if (s.equals(\"a\")) {\n            a();\n        } else if (s.equals(\"b\")) {\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        let out = applied(src, "if (s.equals");
        assert!(out.contains("switch (s) {"), "{out}");
        assert!(out.contains("case \"a\": {"), "{out}");
    }

    /// `"a".equals(s)` survives a null `s`; `switch (s)` does not.
    #[test]
    fn the_null_safe_idiom_is_left_alone() {
        let src = "class A {\n    void f(String s) {\n        if (\"a\".equals(s)) {\n            a();\n        } else if (\"b\".equals(s)) {\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        assert!(outcome(src, "if (\"a\"").is_none());
    }

    /// A subject that is a call ran once per rung and would now run once.
    #[test]
    fn a_call_is_not_a_subject() {
        let src = "class A {\n    void f() {\n        if (kind() == 1) {\n            a();\n        } else if (kind() == 2) {\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        assert!(outcome(src, "if (kind()").is_none());
    }

    #[test]
    fn two_subjects_are_refused() {
        let src = "class A {\n    void f(int a, int b) {\n        if (a == 1) {\n            x();\n        } else if (b == 2) {\n            y();\n        } else {\n            z();\n        }\n    }\n}";
        assert!(refusal(src, "if (a == 1)").contains("two different things"));
    }

    #[test]
    fn a_repeated_label_is_refused() {
        let src = "class A {\n    void f(int k) {\n        if (k == 1) {\n            x();\n        } else if (k == 1) {\n            y();\n        } else {\n            z();\n        }\n    }\n}";
        assert!(refusal(src, "if (k == 1)").contains("only label an arm once"));
    }

    /// A body whose completion cannot be read makes the whole conversion a refusal — see the
    /// module docs on why neither default is acceptable.
    #[test]
    fn a_body_ending_in_a_try_is_refused() {
        let src = "class A {\n    void f(int k) {\n        if (k == 1) {\n            try {\n                a();\n            } finally {\n                b();\n            }\n        } else if (k == 2) {\n            c();\n        } else {\n            d();\n        }\n    }\n}";
        assert!(refusal(src, "if (k == 1)").contains("unreachable"));
    }

    /// Two branches are an `if`/`else`, and a row about them in every menu is noise.
    #[test]
    fn a_two_branch_if_is_silent() {
        let src = "class A {\n    void f(int k) {\n        if (k == 1) {\n            a();\n        } else {\n            b();\n        }\n    }\n}";
        assert!(outcome(src, "if (k == 1)").is_none());
    }

    /// Standing in the guarded code is not standing on the guard.
    #[test]
    fn a_caret_inside_a_body_offers_nothing() {
        let src = "class A {\n    void f(int k) {\n        if (k == 1) {\n            work();\n        } else if (k == 2) {\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        let tree = parse_java(src).unwrap();
        let at = src.find("work").unwrap();
        assert!(if_chain_to_switch(tree.root_node(), src, at, at).is_none());
    }
}
