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
    let imports = single_type_imports(nodes, bytes);
    for &n in nodes {
        match n.kind() {
            // A class/enum/record carries its interface list under `implements` (`interfaces` field).
            "class_declaration" | "enum_declaration" | "record_declaration" => {
                check_list(implements_types(n, bytes), &imports, &mut out);
            }
            // An interface carries its interface list under `extends A, B` (`extends_interfaces`).
            "interface_declaration" => {
                check_list(extends_types(n, bytes), &imports, &mut out);
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
fn check_list(types: Vec<(String, Node)>, imports: &HashMap<String, String>, out: &mut Vec<Diagnostic>) {
    // simple name → what was written before it: (qualifier, whether that occurrence had type args).
    let mut seen: HashMap<String, Vec<(String, FirstSeen)>> = HashMap::new();
    for (text, node) in types {
        let Some(simple) = erased_simple_name(&text) else { continue };
        let qualifier = erased_qualifier(&text, imports);
        let has_args = text.contains('<');
        let earlier = seen.entry(simple.clone()).or_default();
        // The same name under two DIFFERENT qualifiers is two types — `Encoder.Text` and
        // `Decoder.Text`, `RouterFunctions.Visitor` and `RequestPredicates.Visitor` — and only an
        // unqualified spelling (which may be either, or the imported one) cannot be told apart.
        match earlier.iter().find(|(q, _)| q.is_empty() || qualifier.is_empty() || *q == qualifier) {
            None => earlier.push((qualifier, FirstSeen { has_args })),
            Some((_, first)) => {
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

/// What qualifies a written type, as far as the file says — `java.util` in `java.util.List<String>`,
/// `javax.websocket.Encoder` in `Encoder.Text` when `Encoder` is imported, and for a bare `Validator`
/// the package its single-type import names. Empty when the file does not say (a bare name from the
/// own package or `java.lang`, an `Outer.Inner` of an unimported `Outer`).
fn erased_qualifier(text: &str, imports: &HashMap<String, String>) -> String {
    let erased: String = text.split('<').next().unwrap_or(text).split_whitespace().collect();
    let (first, rest) = match erased.split_once('.') {
        Some((first, rest)) => (first, Some(rest)),
        None => (erased.as_str(), None),
    };
    let Some(imported) = imports.get(first) else {
        // Written out in full, or an unimported name.
        return match erased.rsplit_once('.') {
            Some((qualifier, _)) => qualifier.to_string(),
            None => String::new(),
        };
    };
    // `Encoder.Text` with `import javax.websocket.Encoder;` is `javax.websocket.Encoder.Text`.
    let full = match rest {
        Some(rest) => format!("{imported}.{rest}"),
        None => imported.clone(),
    };
    full.rsplit_once('.').map(|(q, _)| q.to_string()).unwrap_or_default()
}

/// `simple name → fully-qualified name` of every `import a.b.C;` of the file (no static, no `*`).
fn single_type_imports(nodes: &[Node], bytes: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for n in nodes.iter().filter(|n| n.kind() == "import_declaration") {
        let Ok(text) = n.utf8_text(bytes) else { continue };
        let path: String = text.trim().trim_start_matches("import").trim().trim_end_matches(';').split_whitespace().collect();
        if path.starts_with("static") || path.ends_with('*') {
            continue;
        }
        if let Some((_, simple)) = path.rsplit_once('.') {
            out.insert(simple.to_string(), path.clone());
        }
    }
    out
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
                if is_class_type_node(ch.kind()) {
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

/// Whether a kind is a REFERENCE type as written — the only thing an `extends`, `implements` or
/// `throws` list can hold. Primitives and arrays are excluded on purpose; see
/// `erasure_clash::is_written_type_node` for the predicate that includes them.
fn is_class_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic { message, severity: crate::engine::check_id::CheckId::DuplicateInterface.severity().to_string(), code: crate::engine::check_id::CheckId::DuplicateInterface.code().to_string(), start: node.start_byte(), end: node.end_byte() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn run(src: &str) -> Vec<Diagnostic> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        iface_dup_errors_nodes(&crate::engine::check::collect_nodes(tree.root_node()), src)
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
    fn one_name_under_two_qualifiers_is_two_interfaces() {
        assert!(run("class C implements Encoder.Text<A>, Decoder.Text<A> {}").is_empty());
        assert!(run("class C implements Router.Visitor, Predicates.Visitor {}").is_empty());
        assert_eq!(msgs("class C implements Router.Visitor, Router.Visitor {}").len(), 1);
        // A bare name could be either, so it still counts against the qualified one.
        assert_eq!(msgs("class C implements Router.Visitor, Visitor {}").len(), 1);
    }

    #[test]
    fn an_import_tells_a_bare_name_from_a_qualified_one() {
        let src = "import org.spring.Validator;\nclass C implements jakarta.validation.Validator, Validator {}";
        assert!(run(src).is_empty());
        // The same package both ways is the same interface.
        let same = "import org.spring.Validator;\nclass C implements org.spring.Validator, Validator {}";
        assert_eq!(msgs(same).len(), 1);
        // And an unimported bare name could be anyone's, so it stays against the qualified one.
        assert_eq!(msgs("class C implements jakarta.validation.Validator, Validator {}").len(), 1);
        let nested = "import javax.websocket.Encoder;\nimport javax.websocket.Decoder;\nclass C implements Encoder.Text<A>, Decoder.Text<A> {}";
        assert!(run(nested).is_empty());
    }

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
