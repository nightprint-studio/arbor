//! Incomplete canonical record constructor (pure-AST, `error`).
//!
//! A record's *canonical* constructor is the one whose parameters are exactly the record's header
//! components (same names, same count). If the author writes it out explicitly (a full
//! `constructor_declaration`, not the compact form) they take on the obligation the compiler would
//! otherwise discharge: assigning EVERY component to its field. A canonical constructor that leaves a
//! component unassigned won't compile — `variable <c> not initialized in the canonical constructor`.
//!
//! This mirrors the blank-final scan in [`crate::init_checks`]: we don't do flow analysis, so ANY
//! textual assignment to the component anywhere in the ctor body (even in one branch) counts as
//! "assigned" and suppresses the report. Only a component assigned NOWHERE in the ctor body is flagged.
//!
//! PARAMOUNT: never a false positive. When in doubt, skip. Concretely we SKIP when:
//!   * the type isn't a `record_declaration`, or has no header components / no body;
//!   * there is NO explicit `constructor_declaration` matching the components — the compiler generates
//!     a canonical ctor that assigns all, so nothing to check;
//!   * the only canonical form is a `compact_constructor_declaration` — it auto-assigns every
//!     component at the end, so it can NEVER be incomplete;
//!   * we can't cleanly match a canonical `constructor_declaration` (its param names don't equal the
//!     component set) — a non-canonical ctor has no such obligation;
//!   * the canonical ctor's first statement delegates via `this(...)` — another ctor initializes.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// All incomplete-canonical-record-constructor errors over the shared pre-collected node list (one
/// traversal across all pure-AST checks).
pub fn record_ctor_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "record_declaration" {
            check_record(n, bytes, &mut out);
        }
    }
    out
}

/// Flag every header component of `record` that its explicit canonical constructor never assigns.
fn check_record(record: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // Component names from the record header (`parameters` = a `formal_parameters` node). If the record
    // has no components, there's nothing to assign → skip.
    let components = record_components(record, bytes);
    if components.is_empty() {
        return;
    }
    let component_set: HashSet<&str> = components.iter().map(String::as_str).collect();

    let Some(body) = record.child_by_field_name("body") else { return };

    // Find the explicit CANONICAL constructor: a `constructor_declaration` whose parameter names are
    // exactly the component set (same names, same count). A `compact_constructor_declaration` is never
    // a candidate here — it auto-assigns all components, so it can't be incomplete. If we don't find
    // one, either there's no explicit ctor (compiler generates a complete one) or only non-canonical
    // ctors exist (no obligation) → skip.
    let Some(ctor) = canonical_constructor(body, &component_set, bytes) else { return };

    // Delegation: if the first statement is `this(...)`, another ctor initializes → skip.
    if delegates_via_this(ctor) {
        return;
    }

    let Some(ctor_body) = ctor.child_by_field_name("body") else { return };
    let assigned = collect_assigned_names(ctor_body, bytes);

    // One diagnostic per component assigned NOWHERE, anchored on the ctor name (tight, always present
    // for a canonical ctor).
    let anchor = ctor.child_by_field_name("name").unwrap_or(ctor);
    for comp in &components {
        if !assigned.contains(comp.as_str()) {
            out.push(err(
                format!("Record component `{comp}` is not initialized in the canonical constructor"),
                anchor,
            ));
        }
    }
}

/// The record's header component names, in order (`record R(String name, int level)` → `["name",
/// "level"]`). Reads the `parameters` field (a `formal_parameters` node) and pulls each
/// `formal_parameter`'s `name`.
fn record_components(record: Node, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(params) = record.child_by_field_name("parameters") else { return out };
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        // Only plain `formal_parameter`s are record components. A `spread_parameter` (varargs) can't be
        // a record component in the header, so its absence here never causes a false positive.
        if p.kind() == "formal_parameter" {
            if let Some(name) = p.child_by_field_name("name") {
                if let Ok(t) = name.utf8_text(bytes) {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

/// The explicit canonical `constructor_declaration` directly in `body`: a full constructor whose
/// parameter names equal `component_set` exactly (same names, same count). Returns `None` if no such
/// constructor exists (so a compact ctor, a missing ctor, or only non-canonical ctors all yield `None`
/// → skip).
fn canonical_constructor<'t>(
    body: Node<'t>,
    component_set: &HashSet<&str>,
    bytes: &[u8],
) -> Option<Node<'t>> {
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        // Only a FULL constructor is checkable. `compact_constructor_declaration` is skipped by kind.
        if member.kind() != "constructor_declaration" {
            continue;
        }
        let param_names = constructor_param_names(member, bytes);
        // Canonical ⇔ its parameters are exactly the components: same count AND same name set. The
        // count guard rejects a ctor that repeats a component name; the set-equality rejects any ctor
        // with a different parameter list (an overload has no canonical obligation).
        if param_names.len() == component_set.len()
            && param_names.iter().all(|n| component_set.contains(n.as_str()))
        {
            return Some(member);
        }
    }
    None
}

/// The parameter names of a `constructor_declaration`, in order. Reads its `parameters` field (a
/// `formal_parameters` node) and pulls each `formal_parameter`'s `name`.
fn constructor_param_names(ctor: Node, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(params) = ctor.child_by_field_name("parameters") else { return out };
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        if matches!(p.kind(), "formal_parameter" | "spread_parameter") {
            if let Some(name) = p.child_by_field_name("name") {
                if let Ok(t) = name.utf8_text(bytes) {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

/// Whether the constructor's FIRST statement is a `this(...)` delegation (an
/// `explicit_constructor_invocation` on `this`). A `super(...)` chain is not delegation to another
/// ctor of the SAME record and doesn't initialize components, so only `this(...)` suppresses.
fn delegates_via_this(ctor: Node) -> bool {
    let Some(body) = ctor.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    for stmt in body.named_children(&mut c) {
        if matches!(stmt.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if stmt.kind() != "explicit_constructor_invocation" {
            return false; // first real statement isn't a chain call
        }
        // The invocation's constructor keyword is `this` for delegation, `super` for a super-call.
        // Records have no superclass to chain to, but a defensive check on the `constructor` field text
        // keeps this precise: only `this` is delegation.
        if let Some(kw) = stmt.child_by_field_name("constructor") {
            return kw.kind() == "this";
        }
        // No `constructor` field to read → be conservative and treat the leading chain call as
        // delegation (suppresses rather than risks a false positive).
        return true;
    }
    false
}

/// The set of identifier names assigned anywhere under `body` — bare `x = …`, `this.x = …`, `X.x = …`,
/// and `x++` / `this.x++` update targets. Mirrors [`crate::init_checks::collect_assigned_names`]:
/// over-collecting only ever suppresses a report (safe); under-collecting would risk a false positive.
fn collect_assigned_names(body: Node, bytes: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(node) = stack.pop() {
        let target = match node.kind() {
            "assignment_expression" => node.child_by_field_name("left"),
            "update_expression" => update_operand(node),
            _ => None,
        };
        if let Some(t) = target {
            if let Some(name) = assigned_target_name(t, bytes) {
                names.insert(name);
            }
        }
        let mut cc = node.walk();
        for ch in node.named_children(&mut cc) {
            stack.push(ch);
        }
    }
    names
}

/// The variable/field name an assignment / update target refers to: `x` → `x`; `this.x` / `a.b.x` →
/// `x` (the trailing field). `None` for an array-index or other complex LHS (ignoring those only ever
/// avoids over-suppression, never a false positive).
fn assigned_target_name(target: Node, bytes: &[u8]) -> Option<String> {
    match target.kind() {
        "identifier" => target.utf8_text(bytes).ok().map(str::to_string),
        "field_access" => target
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(bytes).ok())
            .map(str::to_string),
        _ => None,
    }
}

/// The operand node of an `update_expression` (`x++`, `--x`, `this.x++`) — its single named child.
fn update_operand(update: Node) -> Option<Node> {
    let mut c = update.walk();
    for ch in update.named_children(&mut c) {
        return Some(ch);
    }
    None
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic {
        message,
        severity: crate::check_id::CheckId::RecordConstructor.severity().to_string(),
        code: crate::check_id::CheckId::RecordConstructor.code().to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        record_ctor_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    // ── positive ─────────────────────────────────────────────────────────────

    #[test]
    fn canonical_ctor_missing_a_component_is_flagged() {
        let src = "record UnitRecord(String name, int level) { \
                   public UnitRecord(String name, int level) { this.name = name; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Record component `level` is not initialized"), "{d:?}");
    }

    #[test]
    fn multiple_missing_components_each_flagged() {
        let src = "record R(int a, int b, int c) { \
                   R(int a, int b, int c) { this.a = a; } }";
        let d = errs(src);
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(d.iter().any(|m| m.contains("`b`")), "{d:?}");
        assert!(d.iter().any(|m| m.contains("`c`")), "{d:?}");
        assert!(!d.iter().any(|m| m.contains("`a`")), "{d:?}");
    }

    #[test]
    fn bare_name_assignment_counts_as_initialized() {
        // Only `level` is missing; `name = name;` (bare, no `this.`) still counts as assigned.
        let src = "record R(String name, int level) { \
                   R(String name, int level) { name = name; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`level`"), "{d:?}");
    }

    // ── negatives ────────────────────────────────────────────────────────────

    #[test]
    fn canonical_ctor_assigning_all_is_ok() {
        let src = "record UnitRecord(String name, int level) { \
                   public UnitRecord(String name, int level) { this.name = name; this.level = level; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn component_assigned_in_one_branch_is_not_flagged() {
        // ANY textual assignment (even one branch) suppresses — no flow analysis.
        let src = "record R(int x) { R(int x) { if (x > 0) { this.x = x; } } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn compact_canonical_ctor_is_never_flagged() {
        // A compact ctor auto-assigns every component at the end → can't be incomplete.
        let src = "record R(int x) { R { if (x < 0) throw new IllegalArgumentException(); } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn record_with_no_explicit_ctor_is_not_flagged() {
        // Compiler generates a complete canonical ctor → nothing to check.
        assert!(errs("record R(int x, int y) {}").is_empty());
    }

    #[test]
    fn canonical_ctor_delegating_via_this_is_not_flagged() {
        // The canonical ctor delegates to another ctor that initializes → skip.
        let src = "record R(int x, int y) { \
                   R(int x, int y) { this(x, y, 0); } \
                   R(int x, int y, int z) { this.x = x; this.y = y; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_canonical_ctor_is_not_the_canonical_one() {
        // A ctor with different params is NOT canonical → no obligation to assign the components, and
        // no matching canonical ctor exists → skip entirely.
        let src = "record R(int x, int y) { R(int x) { this.x = x; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn same_arity_different_names_is_not_canonical() {
        // Same param count but different names → not the canonical ctor → skip.
        let src = "record R(int x, int y) { R(int a, int b) { this.x = a; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_record_is_ignored() {
        // A plain class ctor that doesn't assign a field is another check's concern, not ours.
        assert!(errs("class C { int x; C(int x) {} }").is_empty());
    }

    #[test]
    fn empty_component_record_is_ignored() {
        assert!(errs("record R() { R() {} }").is_empty());
    }
}
