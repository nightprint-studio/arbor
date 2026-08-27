//! Data flow — the first checks that follow a **value** rather than read a declaration.
//!
//! Everything else in this crate answers a question about a piece of syntax: does this method exist,
//! is this cast possible, does this class implement what it declares. These answer a question about
//! what happens *when the code runs*: this local is null here, so reaching a member on it throws;
//! this condition already has an answer; this assignment is overwritten before anything reads it.
//!
//! ## Deliberately a small model
//!
//! It is straight-line and method-local, and it **forgets everything** at the first construct it
//! does not model — an `if`, a loop, a `try`, a `switch`. That is not a stage on the way to a real
//! solver; it is the shape that makes the crate's cardinal rule cheap to keep. A flow analysis that
//! is wrong is worse than one that is narrow: it accuses working code of throwing, and the reader
//! has no way to see why except to reconstruct the analysis in their head.
//!
//! So the answer to "why doesn't it catch X" is usually "because a branch happened", and that is on
//! purpose. What it does catch is the shape that survives review precisely because it looks
//! deliberate:
//!
//! ```text
//! Order order = null;
//! …
//! order.getId();          // null-dereference
//! ```
//!
//! ## What is tracked, and what is never
//!
//! **Locals only.** A field can be changed by another method or another thread between two lines,
//! so nothing about one is ever "definite". A local cannot: Java is pass-by-value, so handing it to
//! a call cannot rebind it, and a local captured by a lambda is effectively final and cannot be
//! reassigned at all. That is what makes a straight-line read of a block sound without any of the
//! machinery a real solver needs.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;

/// What is definitely true of a local at a point in a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fact {
    Null,
    NonNull,
}

/// A value written to a local that nothing has read yet.
struct PendingStore {
    /// The span to report — the assignment, not the whole statement.
    span: (usize, usize),
}

/// Every data-flow finding in `root`.
pub fn dataflow_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
        // A method or constructor body is the unit: locals live inside one, and nothing about a
        // local survives leaving it.
        if !matches!(n.kind(), "method_declaration" | "constructor_declaration") {
            continue;
        }
        if let Some(body) = n.child_by_field_name("body") {
            analyze_block(body, bytes, &mut out);
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// Read one block's direct statements in order, with a state that is cleared by anything unmodelled.
fn analyze_block(block: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut facts: HashMap<String, Fact> = HashMap::new();
    let mut pending: HashMap<String, PendingStore> = HashMap::new();

    let mut c = block.walk();
    for stmt in block.named_children(&mut c) {
        match stmt.kind() {
            "local_variable_declaration" => {
                read_pass(stmt, bytes, &facts, &mut pending, out);
                declare(stmt, bytes, &mut facts, &mut pending, out);
            }
            "expression_statement" | "return_statement" | "throw_statement" => {
                read_pass(stmt, bytes, &facts, &mut pending, out);
                assign(stmt, bytes, &mut facts, &mut pending, out);
            }
            // The condition is evaluated in the state we have; the branches are not, so everything
            // is forgotten afterwards. A `while`/`for` condition is deliberately NOT judged: it is
            // re-evaluated with whatever the body did, which this model does not track.
            "if_statement" => {
                if let Some(cond) = stmt.child_by_field_name("condition") {
                    constant_condition(cond, bytes, &facts, out);
                    read_pass(cond, bytes, &facts, &mut pending, out);
                }
                descend(stmt, bytes, out);
                facts.clear();
                pending.clear();
            }
            // Anything else: look inside it for findings of its own, then forget. A loop body runs an
            // unknown number of times, a `try` can jump out of the middle, a `switch` picks a path —
            // none of them leave a state a straight-line read may rely on.
            _ => {
                descend(stmt, bytes, out);
                facts.clear();
                pending.clear();
            }
        }
    }
}

/// Analyse the blocks nested inside `stmt`, each on its own fresh state.
fn descend(stmt: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mut stack = vec![stmt];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            // A nested method or lambda body is reached by the top-level walk in its own right —
            // going into it here would analyse it twice.
            if matches!(
                child.kind(),
                "method_declaration" | "constructor_declaration" | "lambda_expression"
            ) {
                continue;
            }
            stack.push(child);
        }
        if n.kind() == "block" && n.id() != stmt.id() {
            analyze_block(n, bytes, out);
        }
    }
}

/// Everything `node` reads: kills the pending store of each name, and flags a member reached on a
/// local that is definitely null.
fn read_pass(
    node: Node,
    bytes: &[u8],
    facts: &HashMap<String, Fact>,
    pending: &mut HashMap<String, PendingStore>,
    out: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        // The left-hand side of an assignment is a write, not a read — `x = 1; x = 2;` must not
        // count the second `x` as reading the first.
        let lhs = (n.kind() == "assignment_expression")
            .then(|| n.child_by_field_name("left"))
            .flatten();
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            if lhs.is_some_and(|l| l.id() == child.id()) && child.kind() == "identifier" {
                continue;
            }
            stack.push(child);
        }

        if n.kind() == "identifier" {
            if let Ok(name) = n.utf8_text(bytes) {
                pending.remove(name);
            }
            continue;
        }
        // `x.foo()` / `x.field` where `x` is definitely null.
        if matches!(n.kind(), "method_invocation" | "field_access") {
            let Some(obj) = n.child_by_field_name("object") else { continue };
            if obj.kind() != "identifier" {
                continue;
            }
            let Ok(name) = obj.utf8_text(bytes) else { continue };
            if facts.get(name) == Some(&Fact::Null) {
                out.push(CheckId::NullDereference.at(
                    obj,
                    format!("`{name}` is `null` here — reaching a member on it throws"),
                ));
            }
        }
    }
}

/// `Type x = <value>;` — record what is known of `x`, and open a pending store.
fn declare(
    stmt: Node,
    bytes: &[u8],
    facts: &mut HashMap<String, Fact>,
    pending: &mut HashMap<String, PendingStore>,
    out: &mut Vec<Diagnostic>,
) {
    let mut c = stmt.walk();
    for d in stmt.named_children(&mut c) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = d.child_by_field_name("name") else { continue };
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        match d.child_by_field_name("value") {
            Some(v) => {
                set_fact(facts, name, value_fact(v));
                // A declaration with an initialiser is a store like any other. One with none binds
                // nothing yet, so there is nothing that could be overwritten unread.
                open_store(pending, name, (d.start_byte(), d.end_byte()), out, bytes);
            }
            None => {
                facts.remove(name);
            }
        }
    }
}

/// `x = <value>;` — the same, for a plain assignment.
fn assign(
    stmt: Node,
    bytes: &[u8],
    facts: &mut HashMap<String, Fact>,
    pending: &mut HashMap<String, PendingStore>,
    out: &mut Vec<Diagnostic>,
) {
    let mut c = stmt.walk();
    for e in stmt.named_children(&mut c) {
        if e.kind() != "assignment_expression" {
            continue;
        }
        let (Some(left), Some(right)) =
            (e.child_by_field_name("left"), e.child_by_field_name("right"))
        else {
            continue;
        };
        if left.kind() != "identifier" {
            continue; // `this.x = …` is a field; `a[i] = …` is not a binding
        }
        let Ok(name) = left.utf8_text(bytes) else { continue };
        // Only a plain `=` rebinds. `x += y` reads x first, which `read_pass` already saw.
        let is_plain = e
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(bytes).ok())
            .map(|op| op == "=")
            .unwrap_or_else(|| {
                // Some grammar builds have no `operator` field; the text between the two sides is
                // the operator, and a compound one is more than a single `=`.
                bytes
                    .get(left.end_byte()..right.start_byte())
                    .and_then(|s| std::str::from_utf8(s).ok())
                    .map(|s| s.trim() == "=")
                    .unwrap_or(false)
            });
        if !is_plain {
            facts.remove(name);
            continue;
        }
        // Only a local we are already tracking: a bare name we never saw declared here is a field,
        // and nothing about a field is ever definite.
        if !facts.contains_key(name) && !pending.contains_key(name) {
            continue;
        }
        set_fact(facts, name, value_fact(right));
        open_store(pending, name, (e.start_byte(), e.end_byte()), out, bytes);
    }
}

fn set_fact(facts: &mut HashMap<String, Fact>, name: &str, fact: Option<Fact>) {
    match fact {
        Some(f) => {
            facts.insert(name.to_string(), f);
        }
        None => {
            facts.remove(name);
        }
    }
}

/// Open a store for `name`, reporting the one it replaces if nothing read it.
fn open_store(
    pending: &mut HashMap<String, PendingStore>,
    name: &str,
    span: (usize, usize),
    out: &mut Vec<Diagnostic>,
    _bytes: &[u8],
) {
    if let Some(prev) = pending.insert(name.to_string(), PendingStore { span }) {
        out.push(CheckId::DeadStore.span(
            prev.span.0,
            prev.span.1,
            format!("The value assigned to `{name}` here is never read — it is overwritten below"),
        ));
    }
}

/// What is definitely true of a value expression, or `None` when nothing is.
fn value_fact(value: Node) -> Option<Fact> {
    match value.kind() {
        "null_literal" => Some(Fact::Null),
        // A construction, a literal and a lambda are never null. Everything else — a call, a field,
        // a ternary, a cast — might be, so nothing is claimed.
        "object_creation_expression"
        | "array_creation_expression"
        | "string_literal"
        | "text_block"
        | "character_literal"
        | "lambda_expression"
        | "method_reference" => Some(Fact::NonNull),
        _ => None,
    }
}

/// `if (x == null)` where `x` is definitely non-null (or the reverse) — the answer is already known.
fn constant_condition(
    cond: Node,
    bytes: &[u8],
    facts: &HashMap<String, Fact>,
    out: &mut Vec<Diagnostic>,
) {
    let inner = if cond.kind() == "parenthesized_expression" {
        match cond.named_child(0) {
            Some(n) => n,
            None => return,
        }
    } else {
        cond
    };
    if inner.kind() != "binary_expression" {
        return;
    }
    let (Some(left), Some(right)) =
        (inner.child_by_field_name("left"), inner.child_by_field_name("right"))
    else {
        return;
    };
    let op = match inner.child_by_field_name("operator").and_then(|o| o.utf8_text(bytes).ok()) {
        Some(o) if o == "==" || o == "!=" => o,
        _ => return,
    };
    // Whichever side is the `null`, the other is the name.
    let (name_node, _) = match (left.kind(), right.kind()) {
        ("null_literal", _) => (right, left),
        (_, "null_literal") => (left, right),
        _ => return,
    };
    if name_node.kind() != "identifier" {
        return;
    }
    let Ok(name) = name_node.utf8_text(bytes) else { return };
    let Some(fact) = facts.get(name) else { return };
    let is_null = *fact == Fact::Null;
    let answer = if op == "==" { is_null } else { !is_null };
    let state = if is_null { "`null`" } else { "never `null`" };
    out.push(CheckId::ConstantCondition.at(
        inner,
        format!("`{name}` is {state} here, so this is always `{answer}`"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{\n    void m() {{\n{body}\n    }}\n}}\n");
        let tree = bennu_java::prelude::parse_java(&src).expect("parse");
        dataflow_errors_in(tree.root_node(), &src)
            .into_iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
    }

    #[test]
    fn a_member_reached_on_a_definitely_null_local_is_flagged() {
        let d = diags("        String s = null;\n        s.length();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].starts_with("null-dereference"), "{d:?}");
    }

    #[test]
    fn a_local_assigned_something_real_is_not_flagged() {
        let d = diags("        String s = \"x\";\n        s.length();");
        assert!(d.is_empty(), "{d:?}");
    }

    /// Reassignment clears the fact — the null was two lines ago and is gone.
    #[test]
    fn a_local_reassigned_to_a_value_is_no_longer_null() {
        let d = diags("        String s = null;\n        s = \"x\";\n        s.length();");
        assert!(
            !d.iter().any(|m| m.starts_with("null-dereference")),
            "{d:?}"
        );
    }

    /// The model forgets at a branch, on purpose. This is the documented limit, asserted so it
    /// stays a decision rather than becoming a surprise.
    #[test]
    fn a_branch_between_the_two_lines_stops_the_claim() {
        let d = diags("        String s = null;\n        if (flag) { s = \"x\"; }\n        s.length();");
        assert!(
            !d.iter().any(|m| m.starts_with("null-dereference")),
            "after a branch nothing is definite: {d:?}"
        );
    }

    /// A field is not a local: another method could have set it between the two lines.
    #[test]
    fn a_field_is_never_definite() {
        let src = "class C {\n    String s = null;\n    void m() {\n        s.length();\n    }\n}\n";
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let d = dataflow_errors_in(tree.root_node(), src);
        assert!(d.is_empty(), "{d:?}");
    }

    // ── constant condition ────────────────────────────────────────────────────

    #[test]
    fn a_null_check_on_something_definitely_non_null_is_flagged() {
        let d = diags("        String s = \"x\";\n        if (s != null) { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("always `true`"), "{d:?}");
    }

    #[test]
    fn a_null_check_on_something_definitely_null_is_flagged() {
        let d = diags("        String s = null;\n        if (s == null) { }");
        assert!(d.iter().any(|m| m.contains("always `true`")), "{d:?}");
    }

    #[test]
    fn a_null_check_on_an_unknown_value_is_silent() {
        let d = diags("        String s = compute();\n        if (s != null) { }");
        assert!(d.is_empty(), "{d:?}");
    }

    // ── dead store ────────────────────────────────────────────────────────────

    #[test]
    fn a_value_overwritten_before_it_is_read_is_flagged() {
        let d = diags("        String s = \"a\";\n        s = \"b\";\n        s.length();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].starts_with("dead-store"), "{d:?}");
    }

    #[test]
    fn a_value_that_is_read_first_is_not_a_dead_store() {
        let d = diags("        String s = \"a\";\n        s.length();\n        s = \"b\";");
        assert!(!d.iter().any(|m| m.starts_with("dead-store")), "{d:?}");
    }

    /// `s = s + "b"` reads the previous value.
    #[test]
    fn a_self_referencing_assignment_reads_the_old_value() {
        let d = diags("        String s = \"a\";\n        s = s + \"b\";");
        assert!(!d.iter().any(|m| m.starts_with("dead-store")), "{d:?}");
    }

    /// A branch between the two stores could have read it.
    #[test]
    fn a_branch_between_two_stores_stops_the_claim() {
        let d = diags("        String s = \"a\";\n        if (flag) { use(s); }\n        s = \"b\";");
        assert!(!d.iter().any(|m| m.starts_with("dead-store")), "{d:?}");
    }

    /// Two different locals are not each other's dead stores.
    #[test]
    fn two_locals_do_not_interfere() {
        let d = diags("        String a = \"1\";\n        String b = \"2\";\n        a.length();\n        b.length();");
        assert!(d.is_empty(), "{d:?}");
    }

    /// A method with nothing in it produces nothing, and does not panic.
    #[test]
    fn an_empty_method_is_silent() {
        assert!(diags("").is_empty());
    }
}
