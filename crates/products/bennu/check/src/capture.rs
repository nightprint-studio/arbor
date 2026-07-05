//! Captured-variable effective-finality diagnostics (pure-AST).
//!
//! A local captured by a lambda OR an anonymous/inner class must be *effectively final* (JLS §4.12.4 /
//! §15.27.2). [`crate::lambdas`] covers one direction — mutating a captured local from INSIDE a lambda
//! body. This module covers the other, symmetric direction, for BOTH closure kinds: a local that is
//! **captured** by a closure and then **reassigned in its declaring method** is not effectively final
//! (`int c = 0; Runnable r = () -> use(c); c = 5;` → error).
//!
//! PARAMOUNT — never a false positive. Proving "effectively final" in general needs full
//! definite-assignment analysis (a local declared WITHOUT an initializer may be assigned exactly once
//! across `if`/`else` branches and still be effectively final). We therefore flag ONLY the
//! DEFINITELY-not-effectively-final case, exactly as [`crate::finals`] does for `final` locals:
//!
//!   * the local is declared WITH an initializer (so it is already definitely assigned — any later
//!     assignment is unambiguously a SECOND assignment, regardless of branches);
//!   * it is declared exactly once in the scope (a shadowed name can't be attributed safely);
//!   * it is reassigned (`=`, `op=`, `++`, `--`) as a bare identifier in the method scope, AFTER its
//!     declaration and OUTSIDE any closure (a mutation inside a closure is `lambdas`' job);
//!   * it is **captured** — referenced by name inside some lambda / anonymous-class body in the same
//!     scope, where that name is not one the closure itself declares (so a shadowing lambda parameter
//!     is never mistaken for a capture).
//!
//! Every other shape (no initializer, no capture, an array/field element target `v[i] = …` / `v.f =
//! …` that doesn't rebind `v`, a name declared twice) is left alone.

use std::collections::{HashMap, HashSet};

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// All captured-then-reassigned errors over the shared pre-collected node list (one traversal across
/// all pure-AST checks). Each matched scope drives a bounded sub-walk of its own body.
pub fn capture_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_declaration" | "constructor_declaration" => {
                if let Some(body) = n.child_by_field_name("body") {
                    if body.kind() == "block" {
                        check_scope(body, bytes, &mut out);
                    }
                }
            }
            "static_initializer" => {
                let mut c = n.walk();
                for ch in n.named_children(&mut c) {
                    if ch.kind() == "block" {
                        check_scope(ch, bytes, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

fn check_scope(scope: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // Pass 1 — over the scope's OWN region (not descending into closures / nested types / nested
    // callables): the initialized locals (name → declaration-end offset), their declaration counts,
    // and every bare-identifier reassignment site.
    let mut decl_count: HashMap<String, usize> = HashMap::new();
    let mut inited: HashMap<String, usize> = HashMap::new();
    let mut reassigns: Vec<(String, Node)> = Vec::new();
    // The closures found at the top level of this scope (not nested inside another closure) — the
    // capture sources.
    let mut closures: Vec<Node> = Vec::new();

    let mut stack: Vec<Node> = Vec::new();
    let mut c = scope.walk();
    for ch in scope.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(n) = stack.pop() {
        // A closure: record its capture-scan root and DON'T descend — its interior belongs to the
        // closure, not this scope (mutations inside are `lambdas`' concern; captures are read below).
        // For an anonymous class the capture source is its `class_body` ONLY — the constructor
        // arguments (`new Foo(x) { … }`) are evaluated in THIS scope and don't capture `x`.
        if let Some(root) = closure_capture_root(n) {
            closures.push(root);
            continue;
        }
        match n.kind() {
            "local_variable_declaration" => {
                let has_init_final = has_final(n, bytes); // a `final` local is `finals`' job → skip it
                let mut dc = n.walk();
                for d in n.named_children(&mut dc) {
                    if d.kind() != "variable_declarator" {
                        continue;
                    }
                    if let Some(name) = decl_name(d, bytes) {
                        *decl_count.entry(name.clone()).or_insert(0) += 1;
                        if !has_init_final && d.child_by_field_name("value").is_some() {
                            inited.insert(name, n.end_byte());
                        }
                    }
                }
            }
            "formal_parameter" | "spread_parameter" | "catch_formal_parameter" => {
                if let Some(name) = n.child_by_field_name("name").and_then(|x| text(x, bytes)) {
                    *decl_count.entry(name).or_insert(0) += 1;
                }
            }
            "assignment_expression" => {
                if let Some((name, node)) = assign_target_name(n, bytes) {
                    reassigns.push((name, node));
                }
            }
            "update_expression" => {
                if let Some((name, node)) = update_target_name(n, bytes) {
                    reassigns.push((name, node));
                }
            }
            _ => {}
        }
        // Don't cross into a nested callable / type — it owns its own locals.
        if is_local_boundary(n.kind()) {
            continue;
        }
        let mut cc = n.walk();
        for ch in n.named_children(&mut cc) {
            stack.push(ch);
        }
    }

    if closures.is_empty() || inited.is_empty() || reassigns.is_empty() {
        return;
    }

    // Pass 2 — the set of names captured (referenced free) by any top-level closure in this scope.
    let mut captured: HashSet<String> = HashSet::new();
    for &closure in &closures {
        collect_free_names(closure, bytes, &mut captured);
    }
    if captured.is_empty() {
        return;
    }

    // Flag each reassignment of an initialized, singly-declared, captured local.
    for (name, node) in reassigns {
        let Some(&decl_end) = inited.get(&name) else { continue };
        if decl_count.get(&name).copied().unwrap_or(0) != 1 {
            continue; // shadowed → can't attribute the assignment safely
        }
        if node.start_byte() <= decl_end {
            continue; // the initializer itself / a forward reference (different binding)
        }
        if !captured.contains(&name) {
            continue; // reassigned but never captured → effectively-final concern doesn't arise
        }
        out.push(Diagnostic {
            message: format!(
                "Local variable `{name}` is captured by a lambda or inner class and cannot be reassigned; captured variables must be final or effectively final"
            ),
            severity: "error".to_string(),
            start: node.start_byte(),
            end: node.end_byte(),
        });
    }
}

/// The names a closure references FREELY — every identifier used in its subtree minus every name the
/// closure itself declares (params, locals, and — for an anonymous class — its fields). A method-call
/// name (`process(v)` → `process`) is excluded so only real value references count. The result
/// over-approximates captures, but subtracting the closure's own declarations means a shadowing
/// parameter/local of the SAME name is never counted as a capture — which is what keeps this sound.
fn collect_free_names(closure: Node, bytes: &[u8], into: &mut HashSet<String>) {
    let mut declared: HashSet<String> = HashSet::new();
    let mut used: HashSet<String> = HashSet::new();

    let mut stack = vec![closure];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable_declarator" => {
                if let Some(name) = decl_name(n, bytes) {
                    declared.insert(name);
                }
            }
            "formal_parameter" | "spread_parameter" | "catch_formal_parameter" => {
                if let Some(name) = n.child_by_field_name("name").and_then(|x| text(x, bytes)) {
                    declared.insert(name);
                }
            }
            // A bare-identifier lambda parameter (`x -> …`) or an `inferred_parameters` list (`(x, y)
            // -> …`): the identifiers directly under the lambda's `parameters` are declarations.
            "inferred_parameters" => {
                let mut c = n.walk();
                for ch in n.named_children(&mut c) {
                    if ch.kind() == "identifier" {
                        if let Some(t) = text(ch, bytes) {
                            declared.insert(t);
                        }
                    }
                }
            }
            "identifier" => {
                if is_value_reference(n) {
                    if let Some(t) = text(n, bytes) {
                        used.insert(t);
                    }
                }
            }
            _ => {}
        }
        // A single bare-identifier lambda parameter is stored in the lambda's `parameters` field as an
        // `identifier` (handled above as a "value reference" — so we must subtract it as a declaration).
        if n.kind() == "lambda_expression" {
            if let Some(params) = n.child_by_field_name("parameters") {
                if params.kind() == "identifier" {
                    if let Some(t) = text(params, bytes) {
                        declared.insert(t);
                    }
                }
            }
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
    }

    for name in used {
        if !declared.contains(&name) {
            into.insert(name);
        }
    }
}

/// Whether an `identifier` node is a value reference we care about (a variable read), as opposed to a
/// method-invocation name (`foo` in `foo(x)`) or a member selector. We keep it simple: exclude the
/// `name` field of a `method_invocation`; everything else that is a bare `identifier` counts.
fn is_value_reference(id: Node) -> bool {
    let Some(parent) = id.parent() else { return true };
    if parent.kind() == "method_invocation" {
        // The invoked method's own name is not a captured variable; its receiver / arguments are.
        if parent.child_by_field_name("name").map(|n| n.id()) == Some(id.id()) {
            return false;
        }
    }
    true
}

/// If `n` is a closure, the subtree to scan for captured names: a lambda (whole node — its params are
/// subtracted as declarations), or the `class_body` of an anonymous-class instantiation (`new T(...) {
/// … }`) — NOT the surrounding constructor arguments, which belong to the enclosing scope. `None` when
/// `n` isn't a closure.
fn closure_capture_root(n: Node) -> Option<Node> {
    match n.kind() {
        "lambda_expression" => Some(n),
        "object_creation_expression" => {
            let mut c = n.walk();
            // Explicit loop, never `.find` on `named_children` (cursor-lifetime borrow gotcha).
            let mut found = None;
            for ch in n.named_children(&mut c) {
                if ch.kind() == "class_body" {
                    found = Some(ch);
                    break;
                }
            }
            found
        }
        _ => None,
    }
}

/// A node that owns its own locals — we must not cross it when scanning ONE scope's names. (Closures
/// are handled separately in [`check_scope`] so they can double as capture sources.)
fn is_local_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
            | "method_declaration"
            | "constructor_declaration"
    )
}

fn has_final(node: Node, bytes: &[u8]) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if m.utf8_text(bytes) == Ok("final") {
                    return true;
                }
            }
        }
    }
    false
}

fn text(node: Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(str::to_string)
}

fn decl_name(declarator: Node, bytes: &[u8]) -> Option<String> {
    declarator.child_by_field_name("name").and_then(|n| text(n, bytes))
}

/// The bare-identifier target of an assignment (`x = …`), else `None` (`this.x` / `a[i]` / `o.f` LHS
/// does NOT rebind `x`, so it never breaks effective finality).
fn assign_target_name<'t>(assign: Node<'t>, bytes: &[u8]) -> Option<(String, Node<'t>)> {
    let left = assign.child_by_field_name("left")?;
    if left.kind() == "identifier" {
        return text(left, bytes).map(|s| (s, left));
    }
    None
}

/// The bare-identifier operand of an update expression (`x++`, `--x`).
fn update_target_name<'t>(update: Node<'t>, bytes: &[u8]) -> Option<(String, Node<'t>)> {
    let mut c = update.walk();
    for ch in update.named_children(&mut c) {
        if ch.kind() == "identifier" {
            return text(ch, bytes).map(|s| (s, ch));
        }
        return None; // the only named child is the operand; a non-identifier operand doesn't rebind
    }
    None
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

    fn errs(members: &str) -> Vec<String> {
        let src = format!("import java.util.function.Supplier; class C {{ {members} }}");
        let tree = parse(&src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        capture_errors_nodes(&nodes, &src).into_iter().map(|d| d.message).collect()
    }

    // ── positives ──────────────────────────────────────────────────────────────

    #[test]
    fn captured_local_reassigned_after_lambda_is_flagged() {
        // The prova-bennu lambda case: capture `counter`, then `counter++` in the method.
        let e = errs("void m() { int counter = 0; Supplier<Integer> s = () -> counter; counter++; }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`counter`") && e[0].contains("effectively final"), "{e:?}");
    }

    #[test]
    fn captured_local_reassigned_after_anonymous_class_is_flagged() {
        // The prova-bennu anonymous-class case: capture `total` in a Runnable, then `total = 200;`.
        let src = "void m() { int total = 100; Runnable r = new Runnable() { public void run() { System.out.println(total); } }; total = 200; }";
        let e = errs(src);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`total`"), "{e:?}");
    }

    #[test]
    fn plain_assignment_form_is_flagged() {
        let e = errs("void m() { int x = 1; Supplier<Integer> s = () -> x; x = 2; }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`x`"), "{e:?}");
    }

    // ── negatives (must NEVER flag) ────────────────────────────────────────────

    #[test]
    fn captured_but_never_reassigned_is_ok() {
        // Effectively final: captured, never mutated → legal.
        assert!(errs("void m() { int c = 0; Supplier<Integer> s = () -> c; }").is_empty());
    }

    #[test]
    fn reassigned_but_never_captured_is_ok() {
        // Mutated but not captured by any closure → effective finality doesn't apply.
        assert!(errs("void m() { int c = 0; c++; c = 5; }").is_empty());
    }

    #[test]
    fn lambda_parameter_shadowing_is_not_a_capture() {
        // The lambda's own parameter `x` shadows the local `x`; the local is not captured, so
        // reassigning it is legal → must NOT flag.
        let src = "void m() { int x = 0; java.util.function.Function<Integer,Integer> f = x -> x + 1; x = 5; }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn local_without_initializer_is_not_flagged() {
        // No initializer → assigned once later could be effectively final (definite-assignment) → skip.
        let src = "void m() { int c; c = 1; Supplier<Integer> s = () -> c; }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn shadowed_local_is_not_flagged() {
        let src = "void m() { int c = 0; Supplier<Integer> s = () -> c; { int c = 9; c = 8; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn field_target_does_not_break_finality() {
        // `this.c = …` rebinds a field, not the captured local — legal.
        let src = "int c; void m() { int local = 0; Supplier<Integer> s = () -> local; this.c = 5; }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn final_local_is_left_to_finals_check() {
        // A `final` local reassignment is `finals`' error, not ours — we must not also report it.
        assert!(errs("void m() { final int c = 0; Supplier<Integer> s = () -> c; }").is_empty());
    }

    #[test]
    fn effectively_final_capture_is_clean() {
        let src = "void m() { int a = 1, b = 2; Supplier<Integer> s = () -> a + b; }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }
}
