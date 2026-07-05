//! Duplicate-interface diagnostics — a *pure-AST* check (no resolver): a class whose `implements`
//! clause (or an interface whose `extends` clause) lists the **same interface twice**.
//!
//! Two shapes, one error family, distinguished only by their message:
//!
//!   * **different type arguments** — `class C implements List<String>, List<Integer>`. The compiler
//!     rejects "interface `List` cannot be inherited with different type arguments".
//!   * **plain duplicate** — `class C implements Foo, Foo` / `interface I extends A, A`. Listing the
//!     same interface twice is itself an error, whether or not the (absent) type args differ.
//!
//! This is purely syntactic: we group the written type entries by their **erased simple name** (strip
//! everything from the first `<`, then take the last `.`-separated segment: `java.util.List<String>`
//! → `List`). No resolution is done — `java.util.List` and a bare `List` collide by simple name, which
//! matches how javac reports the clash for the common single-import case and stays conservative
//! (see `simple_erased_matches_qualified` in the tests, and the note there on this deliberate choice).
//!
//! Soundness (docs: NEVER a false positive):
//!   * an entry whose type node doesn't yield a clean simple name is **skipped** — never grouped, so a
//!     malformed / unparseable type can't manufacture a collision;
//!   * we compare only *within one class's own* list; nothing about the class's real supertypes.
//!
//! Only the **second** occurrence of a colliding simple name is flagged (the first is the "keeper"),
//! reporting that entry's type node. A third repeat flags again against the first — every extra
//! listing past the first is an error.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag classes/interfaces that list the same interface more than once in their
/// `implements` / `extends` clause. Pure-AST: iterates the shared `nodes` slice, no resolver.
pub fn iface_dup_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // A class/enum/record carries its interface list under `implements` (`interfaces` field).
            "class_declaration" | "enum_declaration" | "record_declaration" => {
                check_list(implements_types(n, bytes), &mut out);
            }
            // An interface carries its interface list under `extends A, B` (`extends_interfaces`).
            "interface_declaration" => {
                check_list(extends_types(n, bytes), &mut out);
            }
            _ => {}
        }
    }
    out
}

/// Walk one clause's `(text, node)` entries in written order; the first time a given erased simple
/// name is seen we remember whether its text carried type arguments; every LATER entry with the same
/// simple name is flagged against that first sighting. Skips any entry without a clean simple name so
/// an unparseable type can never fabricate (or hide) a collision.
fn check_list(types: Vec<(String, Node)>, out: &mut Vec<Diagnostic>) {
    // simple name → (text of the first occurrence, whether that first occurrence had type args).
    let mut seen: HashMap<String, FirstSeen> = HashMap::new();
    for (text, node) in types {
        let Some(simple) = erased_simple_name(&text) else { continue };
        let has_args = text.contains('<');
        match seen.get(&simple) {
            None => {
                seen.insert(simple, FirstSeen { has_args });
            }
            Some(first) => {
                // A duplicate. If EITHER the first or this occurrence carries type arguments, the two
                // written forms differ in their (possibly-empty) argument lists, so we use the
                // "different type arguments" wording; otherwise it's a plain identical duplicate.
                let differing_args = first.has_args || has_args;
                let message = if differing_args {
                    format!("Interface `{simple}` is listed more than once (with different type arguments)")
                } else {
                    format!("Duplicate interface `{simple}` in the implements/extends list")
                };
                out.push(err(message, node));
            }
        }
    }
}

/// The first-seen record for one erased simple name within a clause.
struct FirstSeen {
    has_args: bool,
}

/// The erased simple name of a written type: strip everything from the first `<` (type arguments),
/// trim, then take the last `.`-separated segment (`java.util.List<String>` → `List`, `List` →
/// `List`, `Outer.Inner` → `Inner`). Returns `None` when the result is empty, so the caller SKIPS the
/// entry rather than grouping a blank name.
fn erased_simple_name(text: &str) -> Option<String> {
    // Erase type arguments: everything from the first '<' onward is generics, not the raw name.
    let erased = match text.find('<') {
        Some(i) => &text[..i],
        None => text,
    };
    let erased = erased.trim();
    // The simple name is the last dotted segment (drop any package / enclosing-type qualifier).
    let simple = erased.rsplit('.').next().unwrap_or(erased).trim();
    if simple.is_empty() {
        None
    } else {
        Some(simple.to_string())
    }
}

// ── CST helpers (same traversal shape as `inheritance.rs`) ───────────────────

/// The `implements` types of a class (`interfaces` field → `type_list`).
fn implements_types<'t>(n: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    match n.child_by_field_name("interfaces") {
        Some(w) => types_under_list(w, bytes),
        None => Vec::new(),
    }
}

/// The `extends` types of an interface (`extends_interfaces` → `type_list`).
fn extends_types<'t>(n: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == "extends_interfaces" {
            return types_under_list(ch, bytes);
        }
    }
    Vec::new()
}

/// Every type node under a `type_list` (possibly nested in `interfaces` / `extends_interfaces`), as
/// `(text, node)` in written order — a verbatim reuse of `inheritance.rs`'s traversal.
fn types_under_list<'t>(wrapper: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    let mut out = Vec::new();
    let mut stack = vec![wrapper];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_list" || node.kind() == "interface_type_list" {
            let mut c = node.walk();
            for ch in node.named_children(&mut c) {
                if is_type_node(ch.kind()) {
                    if let Ok(t) = ch.utf8_text(bytes) {
                        out.push((t.to_string(), ch));
                    }
                }
            }
        } else {
            let mut c = node.walk();
            for ch in node.named_children(&mut c) {
                stack.push(ch);
            }
        }
    }
    out
}

fn is_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic { message, severity: "error".to_string(), start: node.start_byte(), end: node.end_byte() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn run(src: &str) -> Vec<Diagnostic> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        iface_dup_errors_nodes(&crate::check::collect_nodes(tree.root_node()), src)
    }

    fn msgs(src: &str) -> Vec<String> {
        run(src).into_iter().map(|d| d.message).collect()
    }

    // ── erased-name unit ────────────────────────────────────────────────────

    #[test]
    fn erased_simple_name_strips_args_and_package() {
        assert_eq!(erased_simple_name("List").as_deref(), Some("List"));
        assert_eq!(erased_simple_name("List<String>").as_deref(), Some("List"));
        assert_eq!(erased_simple_name("java.util.List<String>").as_deref(), Some("List"));
        assert_eq!(erased_simple_name("Outer.Inner<T>").as_deref(), Some("Inner"));
        // Degenerate inputs never yield a name to group on.
        assert_eq!(erased_simple_name("<String>"), None);
        assert_eq!(erased_simple_name("   "), None);
    }

    // ── positives ───────────────────────────────────────────────────────────

    #[test]
    fn different_type_args_are_flagged() {
        let d = msgs("class C implements List<String>, List<Integer> {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("List") && d[0].contains("different type arguments"), "{d:?}");
    }

    #[test]
    fn interface_extends_plain_duplicate_is_flagged() {
        let d = msgs("interface I extends A, A {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Duplicate interface `A`"), "{d:?}");
    }

    #[test]
    fn triple_listing_flags_each_repeat() {
        // The 2nd and 3rd `Foo` both collide with the first → two diagnostics.
        let d = msgs("class C implements Foo, Foo, Foo {}");
        assert_eq!(d.len(), 2, "{d:?}");
    }

    #[test]
    fn simple_erased_matches_qualified() {
        // DELIBERATE CHOICE: we match by SIMPLE name only, so a fully-qualified `java.util.List` and a
        // bare `List` collide. This mirrors javac for the common single-import case (`import
        // java.util.List;` then `implements java.util.List<String>, List<Integer>` is the same type
        // twice) and stays conservative — it can only under-report if the two simple names genuinely
        // referred to different packages, a shape essentially never written in a real implements list.
        let d = msgs("class C implements java.util.List<String>, List<Integer> {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("different type arguments"), "{d:?}");
    }

    // ── negatives (never a false positive) ──────────────────────────────────

    #[test]
    fn distinct_generic_interfaces_are_ok() {
        assert!(run("class C implements List<String>, Set<String> {}").is_empty());
    }

    #[test]
    fn single_generic_interface_is_ok() {
        assert!(run("class C implements Comparable<C> {}").is_empty());
    }

    #[test]
    fn all_distinct_interfaces_are_ok() {
        assert!(run("class C implements A, B, Cee {}").is_empty());
    }

    #[test]
    fn no_implements_clause_is_ok() {
        assert!(run("class C {}").is_empty());
        assert!(run("class C extends Base {}").is_empty());
    }

    #[test]
    fn distinct_qualified_interfaces_are_ok() {
        // Different simple names, even both qualified → no collision.
        assert!(run("class C implements java.util.List<String>, java.util.Set<String> {}").is_empty());
    }
}
