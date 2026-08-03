//! Duplicate `case` label within one `switch` (pure-AST, **error**).
//!
//! Java rejects a `switch` that lists the same constant in two `case` labels
//! (`switch (f) { case ROME -> …; case CARTHAGE -> …; case ROME -> …; }`). This is a hard compile
//! error — the second `ROME` is unreachable duplicate — and it's decidable from the AST alone: the
//! label constants are literal syntax, so no resolver / inference is needed.
//!
//! ## Never a false positive (the paramount rule)
//! We compare labels by their **trimmed source text**, scoped to ONE switch:
//!   * a switch's labels are collected from its DIRECT body arms only — a nested `switch` is a
//!     separate `switch_expression` node with its own body, so its labels never leak into the outer
//!     switch's set (and vice-versa). Each switch is checked against itself alone.
//!   * `default` is skipped (it is not a constant label; duplicate `default` is a different error we
//!     don't touch).
//!   * a **pattern** label is skipped entirely — see below.
//!   * two labels are a duplicate only when their trimmed text is byte-for-byte equal. Distinct texts
//!     are never flagged — so `case A` vs `case B`, or `Foo.A` vs `Bar.A`, stay silent.
//!
//! Textual equality is *sound* for this rule: if two label constants have identical source text they
//! denote the same constant (the compiler would reject them too). The converse — two spellings of the
//! same constant (`A` vs `Cardinal.A`) — we simply don't flag, which only under-reports, never
//! over-reports. We flag the SECOND occurrence (the later label is the redundant one).
//!
//! ## Pattern labels are not constants
//!
//! Java 21's `switch` takes **patterns**, optionally with a `when` guard:
//!
//! ```text
//! case Pair<Boolean, Boolean> p when p.getLeft()  -> "tsd";
//! case Pair<Boolean, Boolean> p when p.getRight() -> "pdf/a";
//! ```
//!
//! Those two arms share a pattern and differ only in their guard, and they are perfectly legal —
//! but by source text the patterns are identical, so a text comparison called the second one a
//! duplicate. Worse, the grammar makes a `guard` a *sibling* of the pattern under `switch_label`,
//! so the guard expression was itself being collected as if it were a case constant.
//!
//! The rule this check implements is about **constants**, and a pattern is not one. What Java
//! forbids between patterns is *dominance* — an earlier pattern that already matches everything a
//! later one could — and that is a different analysis: it needs the subtype relation, and an
//! unguarded pattern dominates where a guarded one dominates nothing. Until that check exists,
//! pattern labels are left alone: silence on a rule we don't implement, rather than a wrong answer
//! borrowed from one we do.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag duplicate `case` labels within a single `switch` over a shared pre-collected node slice (one
/// traversal across all pure-AST checks). Each `switch_expression` is checked independently against
/// its own DIRECT labels, so nested switches never cross-contaminate.
pub fn switch_dup_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        // A statement `switch` and an expression `switch` share the `switch_expression` grammar kind;
        // both are checked. Skip a broken subtree — a parse error there makes the label reads
        // unreliable, and a duplicate flagged on garbage would be a false positive.
        if n.kind() == "switch_expression" && !n.has_error() {
            check_switch(n, bytes, &mut out);
        }
    }
    out
}

/// Collect the constant labels of ONE switch (its direct arms' `switch_label`s, `default` excluded)
/// and flag the second occurrence of any label whose trimmed text repeats.
fn check_switch<'t>(switch: Node<'t>, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = switch.child_by_field_name("body") else { return };

    // Every constant label text seen so far in THIS switch. A label constant repeated → the later one
    // is the duplicate. We keep the trimmed text; comparison is exact-string.
    let mut seen: Vec<String> = Vec::new();

    // body → arm (`switch_rule` | `switch_block_statement_group`) → `switch_label` → constant(s).
    // We descend exactly one level into the body, so a nested switch (which lives INSIDE an arm's
    // statement, not as a direct body child) is never visited here — its labels belong to its own
    // `switch_expression` pass. Explicit `for` loops throughout (never `.find`/`.any` on a cursor).
    let mut bc = body.walk();
    for arm in body.named_children(&mut bc) {
        if !matches!(arm.kind(), "switch_rule" | "switch_block_statement_group") {
            continue;
        }
        let mut ac = arm.walk();
        for label in arm.named_children(&mut ac) {
            if label.kind() != "switch_label" {
                continue;
            }
            // `default` is not a constant label — skip it (a `default` carries no named constant
            // child, but skip explicitly for clarity and to never treat it as a value).
            if label_is_default(label, bytes) {
                continue;
            }
            // A **pattern** label (`case Foo f`, `case Foo f when …`, `case Point(int x, int y)`)
            // is not a constant label — see the module doc. Skip the whole label: its pattern is
            // not comparable by text, and its `guard` is a sibling that must never be mistaken
            // for a case constant.
            if label_is_pattern(label) {
                continue;
            }
            // A `case` label lists one or more constants as its NAMED children (`case A` → one;
            // `case A, B` → two). Treat each constant separately; compare by trimmed text.
            let mut lc = label.walk();
            for cst in label.named_children(&mut lc) {
                let Ok(raw) = cst.utf8_text(bytes) else { continue };
                let text = raw.trim();
                if text.is_empty() {
                    continue;
                }
                if seen.iter().any(|s| s == text) {
                    out.push(Diagnostic {
                        message: format!("Duplicate case label `{text}`"),
                        severity: crate::check_id::CheckId::DuplicateCaseLabel.severity().to_string(),
                        code: crate::check_id::CheckId::DuplicateCaseLabel.code().to_string(),
                        start: cst.start_byte(),
                        end: cst.end_byte(),
                    });
                } else {
                    seen.push(text.to_string());
                }
            }
        }
    }
}

/// Whether a `switch_label` is a **pattern** label rather than a constant one.
///
/// Per tree-sitter-java, a `switch_label`'s named children are one of: an `expression` (the constant
/// forms), a `pattern` (a `type_pattern` or a `record_pattern`), or a `guard` (the `when` clause,
/// which is a sibling of the pattern, not nested inside it). Either of the latter two makes this a
/// pattern label — the guard is checked too so a grammar that ever emitted one without a sibling
/// pattern still can't leak a guard expression into the constant set.
fn label_is_pattern(label: Node) -> bool {
    let mut c = label.walk();
    for ch in label.named_children(&mut c) {
        if matches!(ch.kind(), "pattern" | "type_pattern" | "record_pattern" | "guard") {
            return true;
        }
    }
    false
}

/// Whether a `switch_label` is the `default` clause. The `default` keyword is an anonymous (unnamed)
/// child of the label (no named constant child), so we scan the label's children including anonymous
/// ones for its text — the same shape [`crate::enum_switch`] relies on.
fn label_is_default(label: Node, bytes: &[u8]) -> bool {
    let mut c = label.walk();
    for ch in label.children(&mut c) {
        if !ch.is_named() && ch.utf8_text(bytes) == Ok("default") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn dups(body: &str) -> Vec<String> {
        let src = format!("class C {{ String m(int i, Cardinal f) {{ {body} }} }}");
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let nodes = crate::check::collect_nodes(tree.root_node());
        switch_dup_errors_nodes(&nodes, &src).into_iter().map(|d| d.message).collect()
    }

    // ── positives ────────────────────────────────────────────────────────────────

    #[test]
    fn arrow_form_duplicate_enum_constant_is_flagged() {
        let d = dups(
            "return switch (f) { case ROME -> \"a\"; case CARTHAGE -> \"b\"; case ROME -> \"c\"; };",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0], "Duplicate case label `ROME`", "{d:?}");
    }

    #[test]
    fn colon_form_duplicate_int_literal_is_flagged() {
        let d = dups("switch (i) { case 1: break; case 1: break; } return \"\";");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0], "Duplicate case label `1`", "{d:?}");
    }

    #[test]
    fn duplicate_within_a_multi_constant_label_is_flagged() {
        // `case A, B:` then `case A:` — A repeats across the two labels.
        let d = dups("switch (i) { case 1, 2: break; case 1: break; } return \"\";");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0], "Duplicate case label `1`", "{d:?}");
    }

    // ── negatives (critical — never a false positive) ────────────────────────────

    #[test]
    fn distinct_labels_are_not_flagged() {
        assert!(dups("return switch (f) { case ROME -> \"a\"; case CARTHAGE -> \"b\"; };").is_empty());
    }

    #[test]
    fn multi_constant_labels_without_repeat_are_ok() {
        // `case A, B:` and `case C:` — no constant repeats.
        assert!(dups("switch (i) { case 1, 2: break; case 3: break; } return \"\";").is_empty());
    }

    #[test]
    fn two_separate_switches_each_with_own_labels_are_ok() {
        // Two sibling switches each list ROME once — same text, but DIFFERENT switches → not a dup.
        let d = dups(
            "switch (f) { case ROME -> {} case CARTHAGE -> {} } \
             switch (f) { case ROME -> {} case ATHENS -> {} } return \"\";",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn nested_switch_reusing_outer_label_is_ok() {
        // The inner switch reuses ROME, but it's a distinct switch with its own label set → no dup.
        let d = dups(
            "switch (f) { \
               case ROME -> { switch (f) { case ROME -> {} case ATHENS -> {} } } \
               case CARTHAGE -> {} \
             } return \"\";",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn duplicate_default_is_not_flagged() {
        // Two `default`s is a different error; this check never touches `default`.
        assert!(dups("switch (i) { case 1: break; default: break; } return \"\";").is_empty());
    }

    // ── Java 21 pattern labels (never a duplicate) ───────────────────────────────

    /// The reported bug: two arms sharing a type pattern and differing only in their `when` guard
    /// are legal Java, and were reported as a duplicate case label.
    #[test]
    fn same_pattern_with_different_guards_is_not_a_duplicate() {
        let d = dups(
            "return switch (Pair.of(a, b)) { \
               case Pair<Boolean, Boolean> p when p.getLeft() -> \"tsd\"; \
               case Pair<Boolean, Boolean> p when p.getRight() -> \"pdf/a\"; \
               default -> \"pdf\"; \
             };",
        );
        assert!(d.is_empty(), "a guard distinguishes the arms: {d:?}");
    }

    /// Even with no guard at all, a repeated pattern is a *dominance* question this check does not
    /// answer — it must stay silent rather than borrow the constant rule's verdict.
    #[test]
    fn repeated_patterns_without_guards_are_left_alone() {
        let d = dups(
            "return switch (o) { case String s -> \"a\"; case String s -> \"b\"; default -> \"c\"; };",
        );
        assert!(d.is_empty(), "pattern dominance is not this check's rule: {d:?}");
    }

    /// A record deconstruction pattern is a pattern too.
    #[test]
    fn record_patterns_are_left_alone() {
        let d = dups(
            "return switch (o) { case Point(int x, int y) -> \"a\"; \
               case Point(int x, int y) -> \"b\"; default -> \"c\"; };",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    /// The guard expression must never be collected as a case constant — two arms guarded on the
    /// same expression but matching different types are legal, and the shared guard text used to
    /// look like a repeated label.
    #[test]
    fn a_shared_guard_expression_is_not_a_case_constant() {
        let d = dups(
            "return switch (o) { case String s when flag -> \"a\"; \
               case Integer i when flag -> \"b\"; default -> \"c\"; };",
        );
        assert!(d.is_empty(), "a guard is not a label constant: {d:?}");
    }

    /// And a constant switch in the same file keeps working — the skip is scoped to pattern labels,
    /// not to switches that happen to contain one.
    #[test]
    fn constant_labels_next_to_pattern_labels_still_report() {
        let d = dups("switch (i) { case 1: break; case 1: break; } return \"\";");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn qualified_constants_compared_by_text() {
        // `Foo.A` vs `Bar.A` are distinct texts → not a dup; `Foo.A` twice → dup.
        assert!(dups("switch (i) { case Foo.A: break; case Bar.A: break; } return \"\";").is_empty());
        let d = dups("switch (i) { case Foo.A: break; case Foo.A: break; } return \"\";");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0], "Duplicate case label `Foo.A`", "{d:?}");
    }
}
