//! Recursive-constructor diagnostics (pure-AST, `error`).
//!
//! A set of constructors in one class whose `this(...)` delegations form a cycle — no constructor in
//! the cycle ever bottoms out at a real body or a `super(...)` call, so instantiating via any of them
//! would recurse forever. The simplest guaranteed cases:
//!   * direct self-delegation — `Foo(int x) { this(0); }` (arity 1 delegates to a 1-arg `this(...)`,
//!     i.e. back to itself);
//!   * a 2-cycle — `Foo() { this(0); } Foo(int x) { this(); }` (arity 0 → arity 1 → arity 0).
//!
//! ## Model — an arity-graph, never types
//! Java can't reason about which overload a `this(args)` targets from the argument COUNT alone, and
//! neither can this pure-AST check. We model each constructor by its PARAMETER COUNT (arity) and, if
//! its first statement is a `this(...)` chain call, by the ARGUMENT COUNT of that call — the arity it
//! delegates to. Per class we build a map `arity -> Option<target_arity>`:
//!   * `Some(t)` — the ctor's first statement is a `this(...)` with `t` arguments (delegates to arity
//!     `t`);
//!   * `None` — it bottoms out: first statement is a `super(...)`, or is not an explicit constructor
//!     invocation at all (a real body).
//! A ctor is on a cycle iff following `this → this` edges from its arity revisits that arity.
//!
//! ## SOUNDNESS — never a false positive
//! Delegation is modelled by argument COUNT only, so two constructors of the same arity are
//! indistinguishable as `this(...)` targets. To never mis-flag, we only judge a class whose delegation
//! is UNAMBIGUOUS: **every arity is declared by at most one constructor**. If any two constructors
//! share an arity, we can't tell which overload a `this(n-args)` resolves to → we SKIP the whole class.
//! We also only treat a `this(...)` that is the constructor's FIRST statement as a chain edge (only
//! there is it a valid delegation); anything else bottoms out.
//!
//! ## Every SKIP (no diagnostic)
//!   * grammar handle fails / parse fails → `[]`;
//!   * a node that isn't a `class_declaration` (only classes are inspected);
//!   * a class where two constructors share an arity (ambiguous overload target);
//!   * a constructor whose first statement is a `super(...)` → bottoms out (`None`);
//!   * a constructor whose first statement isn't an `explicit_constructor_invocation` → bottoms out;
//!   * a `this(...)` edge whose target arity isn't declared in the class → dead end (bottoms out).

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag every constructor that participates in a `this(...)` delegation cycle, over the shared
/// pre-collected node list (one traversal across all pure-AST checks).
pub fn ctor_recursion_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "class_declaration" {
            check_class(n, bytes, &mut out);
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// One constructor's shape in the arity-graph.
struct Ctor<'t> {
    /// The constructor's parameter count (its own arity — the graph node key).
    arity: usize,
    /// If the first statement is a `this(...)` chain call, the argument count it delegates to; else
    /// `None` (a `super(...)` first statement or a real body → bottoms out).
    delegates_to: Option<usize>,
    /// The `name` node — where a diagnostic anchors.
    name: Node<'t>,
}

/// Build the class's arity-graph and flag every constructor on a `this→this` cycle. SKIPs (returns
/// early with nothing) whenever the class is ambiguous — see module docs.
fn check_class(class: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = class.child_by_field_name("body") else { return };
    let mut ctors: Vec<Ctor> = Vec::new();
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        if ch.kind() != "constructor_declaration" {
            continue;
        }
        let Some(name) = ch.child_by_field_name("name") else { continue };
        ctors.push(Ctor { arity: ctor_arity(ch), delegates_to: first_this_arg_count(ch, bytes), name });
    }
    if ctors.is_empty() {
        return;
    }

    // AMBIGUITY SKIP: if any two constructors share an arity, a `this(n-args)` can't be resolved to a
    // single overload → we can't soundly judge the class. Bail on the whole class (never guess).
    let mut edge: HashMap<usize, Option<usize>> = HashMap::new();
    for ctor in &ctors {
        if edge.insert(ctor.arity, ctor.delegates_to).is_some() {
            return; // two ctors with the same arity → ambiguous, SKIP the class
        }
    }

    // Now every arity maps to at most one target. A ctor is on a cycle iff walking `this → this` edges
    // from its arity returns to that arity. `edge[a] == Some(t)` follows to `t`; `None` (bottoms out)
    // or a `t` not present in the class (dead-end delegation) ends the walk — no cycle.
    for ctor in &ctors {
        if on_cycle(ctor.arity, &edge) {
            out.push(err(ctor.name));
        }
    }
}

/// Whether following `this → this` edges from `start` ever revisits `start` (a delegation cycle). A
/// step ends the walk when the current arity bottoms out (`None`) or delegates to an arity not
/// declared in the class (a dead end that can't loop back). Bounded by the number of distinct arities.
fn on_cycle(start: usize, edge: &HashMap<usize, Option<usize>>) -> bool {
    let mut cur = start;
    // At most `edge.len()` hops before we'd necessarily revisit a node; the guard caps the walk.
    for _ in 0..=edge.len() {
        match edge.get(&cur) {
            Some(Some(next)) => {
                if *next == start {
                    return true;
                }
                cur = *next;
            }
            // `Some(None)` = bottoms out; `None` = delegates to an undeclared arity (dead end).
            _ => return false,
        }
    }
    false
}

/// A constructor's parameter count (its arity) — the `formal_parameter` / `spread_parameter` children
/// of its `parameters` list.
fn ctor_arity(ctor: Node) -> usize {
    let Some(params) = ctor.child_by_field_name("parameters") else { return 0 };
    let mut n = 0usize;
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        if matches!(p.kind(), "formal_parameter" | "spread_parameter") {
            n += 1;
        }
    }
    n
}

/// If the constructor's FIRST statement is a `this(...)` explicit constructor invocation, the argument
/// count of that call (the arity it delegates to); else `None` (a `super(...)` head, a real body, or
/// no body → bottoms out). Only the first statement can be a valid chain call.
fn first_this_arg_count(ctor: Node, bytes: &[u8]) -> Option<usize> {
    let body = ctor.child_by_field_name("body")?;
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        if matches!(ch.kind(), "line_comment" | "block_comment") {
            continue;
        }
        // The chain call must be the FIRST real statement.
        if ch.kind() != "explicit_constructor_invocation" {
            return None; // real body → bottoms out
        }
        // Distinguish `this(...)` from `super(...)` by the leading keyword token, and — only for a
        // `this(...)` — count the `argument_list` children as the delegated-to arity.
        return this_call_arg_count(ch, bytes);
    }
    None // empty body → bottoms out
}

/// For an `explicit_constructor_invocation`, `Some(arg_count)` if it's a `this(...)` call, else `None`
/// (a `super(...)` call bottoms out). The `this` vs `super` distinction is the leading keyword token.
fn this_call_arg_count(inv: Node, bytes: &[u8]) -> Option<usize> {
    // The first child of the invocation is the `this` / `super` keyword. We compare its text (rather
    // than trust a node kind) so a stray form is treated as "not `this`" → bottoms out (conservative).
    let mut c = inv.walk();
    let mut is_this = false;
    for ch in inv.children(&mut c) {
        // The keyword appears before the `argument_list`; the first token settles it.
        match ch.utf8_text(bytes) {
            Ok("this") => {
                is_this = true;
                break;
            }
            Ok("super") => return None, // super(...) → bottoms out
            _ => {
                // Keep scanning until we hit `this`/`super`; if we reach the argument list first,
                // something is off — bail conservatively.
                if ch.kind() == "argument_list" {
                    break;
                }
            }
        }
    }
    if !is_this {
        return None;
    }
    let args = inv.child_by_field_name("arguments").or_else(|| named_argument_list(inv))?;
    let mut n = 0usize;
    let mut ac = args.walk();
    for a in args.named_children(&mut ac) {
        // `argument_list` named children are the expressions; comments (if any) aren't named here.
        let _ = a;
        n += 1;
    }
    Some(n)
}

/// The `argument_list` child of an explicit constructor invocation, found by kind (the grammar may not
/// expose it via a field on every version).
fn named_argument_list(inv: Node) -> Option<Node> {
    let mut c = inv.walk();
    for ch in inv.named_children(&mut c) {
        if ch.kind() == "argument_list" {
            return Some(ch);
        }
    }
    None
}

fn err(name: Node) -> Diagnostic {
    Diagnostic {
        message: "Recursive constructor invocation".to_string(),
        severity: "error".to_string(),
        start: name.start_byte(),
        end: name.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn diags(src: &str) -> Vec<Diagnostic> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let nodes = crate::check::collect_nodes(tree.root_node());
        ctor_recursion_errors_nodes(&nodes, src)
    }

    fn count(src: &str) -> usize {
        diags(src).iter().filter(|d| d.message == "Recursive constructor invocation").count()
    }

    // --- positives ---

    #[test]
    fn two_cycle_distinct_arities_is_flagged() {
        // arity 0 → this(1-arg) → arity 1 → this(0-arg) → arity 0. Both ctors on the cycle.
        assert_eq!(count("class R { R(){ this(0); } R(int x){ this(); } }"), 2);
    }

    #[test]
    fn direct_self_delegation_is_flagged() {
        // arity 1, first statement `this(0)` = a 1-arg call → back to itself.
        assert_eq!(count("class S { S(int x){ this(0); } }"), 1);
    }

    // --- negatives ---

    #[test]
    fn arity_one_bottoms_out_not_flagged() {
        // arity 0 delegates to arity 1, but arity 1 has a real body → chain terminates.
        assert_eq!(count("class T { T(){ this(0); } T(int x){} }"), 0);
    }

    #[test]
    fn no_cycle_not_flagged() {
        // arity 1 → this(0-arg) → arity 0, which bottoms out. No loop.
        assert_eq!(count("class U { U(){} U(int x){ this(); } }"), 0);
    }

    #[test]
    fn shared_arity_is_skipped_ambiguous() {
        // Two arity-1 ctors: a `this(1-arg)` could target either overload → SKIP the class.
        assert_eq!(count("class W { W(int x){ this(0); } W(String s){ this(\"a\"); } }"), 0);
    }

    #[test]
    fn super_first_statement_bottoms_out() {
        assert_eq!(count("class V { V(){ super(); } }"), 0);
    }

    #[test]
    fn plain_constructors_are_clean() {
        assert_eq!(count("class C { C(){} C(int x){} C(int x, int y){} }"), 0);
    }

    #[test]
    fn three_cycle_all_flagged() {
        // arity 0 → 1 → 2 → 0, distinct arities → all three on the cycle.
        assert_eq!(
            count("class Z { Z(){ this(0); } Z(int a){ this(0,0); } Z(int a, int b){ this(); } }"),
            3,
        );
    }

    #[test]
    fn delegation_to_missing_arity_is_not_flagged() {
        // arity 0's `this(0,0)` targets arity 2, which no ctor declares → dead-end, no cycle.
        assert_eq!(count("class D { D(){ this(0,0); } D(int x){} }"), 0);
    }

    #[test]
    fn empty_source_is_safe() {
        assert!(diags("").is_empty());
    }
}
