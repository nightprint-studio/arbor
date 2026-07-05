//! Generic-erasure clash diagnostics — two methods in the same type body with the **same name, same
//! arity, and the same parameter types AFTER erasure**, while being *distinct in source* (so
//! [`crate::duplicates`] — which compares raw parameter-type text — misses them). Example the compiler
//! rejects: `void f(List<String> a)` and `void f(List<Integer> a)` both erase to `f(List)`; likewise
//! `f(List<String>)` vs the raw `f(List)`.
//!
//! This is the erasure-aware SIBLING of [`crate::duplicates`]: that check flags pairs whose parameter
//! source text is byte-identical (`f(int)`/`f(int)`); this one flags pairs that are DISTINCT in source
//! but IDENTICAL after erasure. The two never double-report a pair — we explicitly skip a group in
//! which every raw signature is identical (that's `duplicates.rs`'s job).
//!
//! # Soundness — never a false positive
//! Erasure is computed *syntactically and conservatively*: we strip a `generic_type`'s
//! `type_arguments` (`List<String>` → `List`), keep primitives and array dimensions verbatim, and
//! keep a varargs marker. Because a bare type-variable parameter (`T`) erases to its declared bound —
//! which is NOT reliably recoverable from the tree — ANY group containing such an ambiguous parameter
//! is skipped entirely rather than guessed. When in doubt we SKIP; under-reporting is acceptable, a
//! wrong diagnostic is not.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

/// Parse `source` and flag generic-erasure method clashes.
pub fn erasure_clash_errors(source: &str) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    match parser.parse(source, None) {
        Some(tree) => erasure_clash_errors_in(tree.root_node(), source),
        None => Vec::new(),
    }
}

/// Tree-driven core (shared with the single-parse `check_file` path).
pub fn erasure_clash_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    erasure_clash_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// A method's per-group entry: the raw parameter-type texts (to detect the byte-identical case that
/// belongs to `duplicates.rs`), the erased signature, whether the erasure was computable, and the name
/// node to report on if this method turns out to clash.
struct MethodSig<'a> {
    /// Raw parameter-type source texts (whitespace-normalised). `None` for a member we must skip.
    raw: Vec<String>,
    /// Erased parameter-type texts. `None` if any parameter was ambiguous (type variable / unknown).
    erased: Option<Vec<String>>,
    name_node: Node<'a>,
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
///
/// Methods are grouped by `(enclosing body node id, name)`: a clash only occurs between overloads of
/// the SAME name in the SAME class body. Within a group we compare erased signatures; a later method
/// whose erased signature matches an earlier one — and whose group is NOT wholly source-identical — is
/// the clash `duplicates.rs` can't see.
pub fn erasure_clash_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Group key: (body id, method name). Value: every same-named method in that body, in source order.
    let mut groups: HashMap<(usize, String), Vec<MethodSig>> = HashMap::new();
    // Preserve first-seen group order so the output is deterministic before the final sort.
    let mut order: Vec<(usize, String)> = Vec::new();

    for &n in nodes {
        if n.kind() != "method_declaration" {
            continue;
        }
        let Some(name_node) = n.child_by_field_name("name") else { continue };
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        let Some(body) = n.parent() else { continue };

        let (raw, erased) = param_signatures(n, bytes);
        let key = (body.id(), name.to_string());
        let entry = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        entry.push(MethodSig { raw, erased, name_node });
    }

    let mut out = Vec::new();
    for key in &order {
        let members = &groups[key];
        // A clash needs at least two same-name methods.
        if members.len() < 2 {
            continue;
        }
        // Skip the group if EVERY raw signature is identical — that byte-identical duplicate is
        // `duplicates.rs`'s responsibility, and re-flagging it here would double-report.
        if all_raw_signatures_identical(members) {
            continue;
        }
        flag_erasure_clashes(members, source, &mut out);
    }

    out.sort_by_key(|d| d.start);
    out
}

/// Whether every member of a same-name group has the exact same raw parameter-type text — the
/// `duplicates.rs` case (which we must NOT re-flag). A member we had to skip (`raw` empty because a
/// parameter was un-extractable) never makes the group "all identical".
fn all_raw_signatures_identical(members: &[MethodSig]) -> bool {
    let first = &members[0].raw;
    members.iter().all(|m| &m.raw == first)
}

/// For each pair of same-name methods, flag the SECOND when their erased signatures are equal, their
/// raw signatures differ (a genuine erasure clash, not the `duplicates.rs` byte-identical case), and
/// both erasures were computable. First-seen-wins: a method is reported once, against the earliest
/// clashing predecessor.
fn flag_erasure_clashes(members: &[MethodSig], source: &str, out: &mut Vec<Diagnostic>) {
    let name = name_text(members, source);
    for i in 1..members.len() {
        let Some(erased_i) = &members[i].erased else { continue };
        for j in 0..i {
            let Some(erased_j) = &members[j].erased else { continue };
            // Same erased signature but DIFFERENT raw text = the clash `duplicates.rs` misses. If the
            // raw text is identical this exact pair is a plain duplicate (already handled elsewhere).
            if erased_i == erased_j && members[i].raw != members[j].raw {
                let node = members[i].name_node;
                out.push(Diagnostic {
                    message: format!(
                        "Method `{name}` clashes with another `{name}`: same signature after generic type erasure"
                    ),
                    severity: "error".to_string(),
                    start: node.start_byte(),
                    end: node.end_byte(),
                });
                break; // Report `i` once, against its first clashing predecessor.
            }
        }
    }
}

/// The method name for messages (all members of a group share it); read from the first name node.
fn name_text<'a>(members: &[MethodSig<'a>], source: &str) -> String {
    members
        .first()
        .and_then(|m| m.name_node.utf8_text(source.as_bytes()).ok())
        .unwrap_or("?")
        .to_string()
}

/// Compute a method's `(raw, erased)` parameter-type texts.
///
/// - `raw`: each parameter's written type (whitespace-normalised), varargs marked — used to tell the
///   erasure clash apart from the byte-identical `duplicates.rs` case.
/// - `erased`: each parameter's type with generic `type_arguments` stripped. `None` if ANY parameter
///   is ambiguous (a bare type variable, or a type text we can't confidently erase) — so such a group
///   is skipped rather than guessed.
fn param_signatures(member: Node, bytes: &[u8]) -> (Vec<String>, Option<Vec<String>>) {
    let Some(params) = member.child_by_field_name("parameters") else {
        return (Vec::new(), Some(Vec::new()));
    };
    let mut raw = Vec::new();
    let mut erased = Vec::new();
    let mut ambiguous = false;
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        let (type_node, varargs) = match p.kind() {
            "formal_parameter" => (p.child_by_field_name("type"), false),
            "spread_parameter" => (spread_type(p), true),
            _ => continue,
        };
        let Some(ty) = type_node else {
            // A parameter whose type node we can't find — don't guess, skip the whole method.
            ambiguous = true;
            continue;
        };
        let Ok(raw_text) = ty.utf8_text(bytes) else {
            ambiguous = true;
            continue;
        };
        let suffix = if varargs { "..." } else { "" };
        raw.push(format!("{}{suffix}", normalize(raw_text)));

        match erase_type(ty, bytes) {
            Some(e) => erased.push(format!("{e}{suffix}")),
            None => ambiguous = true,
        }
    }
    let erased = if ambiguous { None } else { Some(erased) };
    (raw, erased)
}

/// The type node of a `spread_parameter` (`T... xs`) — the first type-shaped child.
fn spread_type(p: Node) -> Option<Node> {
    let mut c = p.walk();
    for ch in p.named_children(&mut c) {
        if is_type_node(ch.kind()) {
            return Some(ch);
        }
    }
    None
}

/// Whether a node kind denotes a Java type usable as a parameter type.
fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "generic_type"
            | "array_type"
            | "type_identifier"
            | "scoped_type_identifier"
            | "integral_type"
            | "floating_point_type"
            | "boolean_type"
            | "void_type"
    )
}

/// Erase one parameter type to its textual erasure, or `None` if it can't be erased with confidence.
///
/// Erasure rules (syntactic, conservative):
/// - `generic_type` (`List<String>`, `Map<K,V>`) → the base name only (`List`, `Map`); its type
///   arguments are dropped.
/// - `array_type` → erase the element type, then re-append the array dimensions (`List<String>[]`
///   → `List[]`, `int[]` → `int[]`).
/// - primitives (`int`, `long`, `boolean`, …) and `void` → kept verbatim.
/// - `scoped_type_identifier` (`java.util.List`) → kept verbatim (a fully-qualified class name is
///   never a type variable; if it carried type arguments it'd be a `generic_type`).
/// - `type_identifier` (a bare simple name): AMBIGUOUS. It may be a real class (`String`) OR a type
///   variable (`T`) whose erasure is its unknown bound. We CANNOT tell reliably from the tree, so we
///   return `None` (skip the method) rather than risk erasing a variable to itself and mis-flagging.
///   This is the deliberate over-cautious cut demanded by "never a false positive".
fn erase_type(ty: Node, bytes: &[u8]) -> Option<String> {
    match ty.kind() {
        "generic_type" => {
            // The base is the non-`type_arguments` child (a `type_identifier` or
            // `scoped_type_identifier`). Its own erasure follows the same rules — but for a base we
            // treat a simple `type_identifier` here as a real generic class name (only classes take
            // `<...>`), so it erases to itself; a scoped name likewise. A nested-generic base can't
            // occur (the outer `<...>` already sits on this node).
            let base = generic_base(ty)?;
            match base.kind() {
                "type_identifier" | "scoped_type_identifier" => {
                    base.utf8_text(bytes).ok().map(normalize)
                }
                // Any other base shape is unexpected — skip rather than guess.
                _ => None,
            }
        }
        "array_type" => {
            // Erase the element type, then re-attach the raw dimensions text.
            let element = ty.child_by_field_name("element")?;
            let dims = ty.child_by_field_name("dimensions")?;
            let element_erased = erase_type(element, bytes)?;
            let dims_text = dims.utf8_text(bytes).ok().map(normalize)?;
            Some(format!("{element_erased}{dims_text}"))
        }
        // Fully-qualified name: never a type variable, keep as written.
        "scoped_type_identifier" => ty.utf8_text(bytes).ok().map(normalize),
        // Primitives / void: erase to themselves.
        "integral_type" | "floating_point_type" | "boolean_type" | "void_type" => {
            ty.utf8_text(bytes).ok().map(normalize)
        }
        // A bare simple name — could be a type variable; we can't know its bound → SKIP.
        "type_identifier" => None,
        // Anything else (annotated types, wildcards where a type is expected, …) → don't guess.
        _ => None,
    }
}

/// The base (name) child of a `generic_type` — the child that is NOT its `type_arguments`.
fn generic_base(generic_ty: Node) -> Option<Node> {
    let mut c = generic_ty.walk();
    for ch in generic_ty.named_children(&mut c) {
        if ch.kind() != "type_arguments" {
            return Some(ch);
        }
    }
    None
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errs(src: &str) -> Vec<String> {
        erasure_clash_errors(src).into_iter().map(|d| d.message).collect()
    }

    // ── Positives ────────────────────────────────────────────────────────────

    #[test]
    fn list_string_vs_list_integer_clashes() {
        let d = errs("class C { void f(java.util.List<String> a) {} void f(java.util.List<Integer> b) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("clashes with another `f`"), "{d:?}");
        assert!(d[0].contains("generic type erasure"), "{d:?}");
    }

    #[test]
    fn map_kv_vs_map_string_integer_clashes() {
        // Both erase to `f(Map)`; the type args differ so it's not the byte-identical duplicate case.
        let d = errs("class C { void f(java.util.Map<Object,Object> a) {} void f(java.util.Map<String,Integer> b) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn generic_vs_raw_same_base_clashes() {
        // `f(List<String>)` vs raw `f(List)` — same erasure `f(List)`, distinct source → flagged.
        let d = errs("class C { void f(java.util.List<String> a) {} void f(java.util.List b) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // ── Negatives ────────────────────────────────────────────────────────────

    #[test]
    fn different_erased_names_not_flagged() {
        // `List<String>` erases to `List`, `Set<String>` to `Set` — different, legal overload.
        assert!(errs("class C { void f(java.util.List<String> a) {} void f(java.util.Set<String> b) {} }").is_empty());
    }

    #[test]
    fn distinct_primitives_not_flagged() {
        // `int` vs `long` — a legal overload, never an erasure clash.
        assert!(errs("class C { void f(int a) {} void f(long b) {} }").is_empty());
    }

    #[test]
    fn byte_identical_generic_is_duplicates_job_not_ours() {
        // `f(List<String>)` twice is byte-identical → belongs to duplicates.rs, NOT flagged here.
        assert!(
            errs("class C { void f(java.util.List<String> a) {} void f(java.util.List<String> b) {} }").is_empty()
        );
    }

    #[test]
    fn overloads_in_different_nested_classes_not_flagged() {
        // Same erased signature but in DIFFERENT bodies — not a clash.
        let src = "class Outer { class A { void f(java.util.List<String> a) {} } \
                   class B { void f(java.util.List<Integer> b) {} } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn single_method_not_flagged() {
        assert!(errs("class C { void f(java.util.List<String> a) {} }").is_empty());
    }

    #[test]
    fn type_variable_params_skipped() {
        // `<T> f(T)` + `<U> f(U)`: both params are bare simple names that MIGHT be type variables with
        // unknown bounds — ambiguous erasure → SKIP, never flagged.
        assert!(errs("class C { <T> void f(T a) {} <U> void f(U b) {} }").is_empty());
    }

    #[test]
    fn type_variable_inside_generic_still_erases_to_base() {
        // The base `List` is a real class (only classes take `<...>`), so `List<T>` and `List<U>` both
        // erase to `List` and DO clash — the type variable lives only in the (dropped) arguments.
        let d = errs("class C { <T> void f(java.util.List<T> a) {} <U> void f(java.util.List<U> b) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn array_erasure_clash_flagged() {
        // `List<String>[]` and `List<Integer>[]` both erase to `List[]`.
        let d = errs("class C { void f(java.util.List<String>[] a) {} void f(java.util.List<Integer>[] b) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn different_arity_not_flagged() {
        assert!(
            errs("class C { void f(java.util.List<String> a) {} void f(java.util.List<Integer> a, int b) {} }")
                .is_empty()
        );
    }
}
