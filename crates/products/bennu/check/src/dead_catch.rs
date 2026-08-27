//! A `catch` for a checked exception the `try` body cannot throw —
//! `compiler.err.except.never.thrown.in.try`.
//!
//! It does not compile, and it is what a refactor leaves behind: the call that threw `SQLException`
//! moved to a repository, and the `catch (SQLException e)` around what is now three lines of
//! arithmetic stayed. Until the next build, the block reads as though something in there still talks
//! to a database.
//!
//! ## Why this check has to be more careful than the others
//!
//! Everything else here reports what it can SEE. This one reports what it can prove is NOT there,
//! and an absence is only as good as the completeness of the search. One call whose receiver does not
//! resolve, and the body could be throwing anything — so [`Thrown::Unknown`] anywhere in it abandons
//! the whole `try`. That is the load-bearing gate, and the reason `throws_of` answers in three states
//! rather than handing back a list.
//!
//! What is abandoned, and why each one:
//!   * **any unresolved call or construction** — see above;
//!   * **`Exception` / `Throwable` in the catch** — always legal, because they cover the unchecked
//!     exceptions any code can throw;
//!   * **an unchecked catch type** — `RuntimeException`, `Error` and their subtypes may be caught
//!     anywhere, thrown or not;
//!   * **a nested `try`** inside the body — an inner `catch` can consume what the outer one is
//!     waiting for, and modelling that is a second flow analysis;
//!   * **`try`-with-resources** — the implicit `close()` throws too, and it is not written anywhere
//!     in the body;
//!   * **a lambda or an anonymous class** in the body — its calls run later, or not at all, and a
//!     checked exception it declares does not reach this `try`.

use bennu_java::prelude::{FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::checked_throw::is_checked;
use crate::resolve::type_binary;
use crate::throws_of::{thrown_by, Thrown};
use crate::walk::{hierarchy_fully_known, reaches};

/// Every `catch` for an exception its `try` cannot throw.
pub fn dead_catch_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        // `try_with_resources_statement` is deliberately absent: the resource's `close()` throws
        // too, and nothing in the body says so.
        if n.kind() == "try_statement" {
            check_try(n, &root, source, bytes, symbols, resolver, cache, &mut out);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn check_try(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(body) = n.child_by_field_name("body") else { return };
    let Some(thrown) = thrown_in(body, root, source, bytes, symbols, resolver, cache) else {
        return;
    };

    let mut cw = n.walk();
    for clause in n.named_children(&mut cw) {
        if clause.kind() != "catch_clause" {
            continue;
        }
        for caught in catch_types(clause, bytes) {
            let Ok(written) = caught.utf8_text(bytes) else { continue };
            let Some(binary) = type_binary(written, symbols, resolver) else { continue };
            // A hierarchy with a gap makes `is_checked`'s negative answers untrustworthy, and this
            // check reasons entirely from negatives.
            if !hierarchy_fully_known(resolver, &binary) || !is_checked(resolver, &binary) {
                continue;
            }
            // `catch (Exception e)` and `catch (Throwable t)` are always legal — they also cover the
            // unchecked exceptions every statement can raise.
            if binary == "java/lang/Exception" || binary == "java/lang/Throwable" {
                continue;
            }
            // Thrown if the body throws it, one of its subtypes (caught by this clause), or one of
            // its supertypes (which this clause narrows — javac allows that).
            let can_arrive = thrown.iter().any(|t| {
                reaches(resolver, t, &binary) || reaches(resolver, &binary, t)
            });
            if !can_arrive {
                out.push(CheckId::UnthrownCatch.at(
                    caught,
                    format!(
                        "`{}` is never thrown in the body of this `try`",
                        simple(&binary)
                    ),
                ));
            }
        }
    }
}

/// Every checked exception the `try` body can throw, or `None` when the body holds something whose
/// throwing we cannot account for — in which case nothing about it may be concluded.
#[allow(clippy::too_many_arguments)]
fn thrown_in(
    body: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<Vec<String>> {
    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        match n.kind() {
            // A nested try can consume what the outer catch is waiting for.
            "try_statement" | "try_with_resources_statement" => return None,
            // Its body runs elsewhere, or later, or never — and either way not inside this `try`.
            "lambda_expression" | "class_body" => continue,
            "throw_statement" => {
                // `throw new E(...)` — the only shape whose thrown type is written down. Anything
                // else (`throw e;`, `throw supplier.get();`) is a value whose type we would have to
                // infer, so we give up rather than under-count.
                let mut tc = n.walk();
                let created = n
                    .named_children(&mut tc)
                    .find(|c| c.kind() == "object_creation_expression");
                let Some(created) = created else { return None };
                let Some(ty) = created.child_by_field_name("type") else { return None };
                let Ok(written) = ty.utf8_text(bytes) else { return None };
                let Some(binary) = type_binary(written, symbols, resolver) else { return None };
                found.push(binary);
            }
            "method_invocation" | "object_creation_expression" => {
                match thrown_by(n, root, source, bytes, symbols, resolver, cache) {
                    // The UPPER bound: concluding something never arrives is only sound over
                    // everything that could. See `throws_of::Throws`.
                    Thrown::Known(_, t) => found.extend(t.possibly),
                    Thrown::Unknown => return None,
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    Some(found)
}

/// The type nodes a `catch` clause names — several for a multi-catch.
fn catch_types<'t>(clause: Node<'t>, _bytes: &[u8]) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut cw = clause.walk();
    for ch in clause.named_children(&mut cw) {
        if ch.kind() != "catch_formal_parameter" {
            continue;
        }
        let mut pc = ch.walk();
        for p in ch.named_children(&mut pc) {
            match p.kind() {
                "catch_type" => {
                    let mut tc = p.walk();
                    for t in p.named_children(&mut tc) {
                        if matches!(
                            t.kind(),
                            "type_identifier" | "scoped_type_identifier" | "generic_type"
                        ) {
                            out.push(t);
                        }
                    }
                }
                "type_identifier" | "scoped_type_identifier" => out.push(p),
                _ => {}
            }
        }
    }
    out
}

fn simple(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{
        ClassFlags, ClassMembers, Import, Member, MemberKind, TypeRef, Visibility,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn method(name: &str, throws: &[&str]) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple("void"),
            params: Vec::new(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Public,
            raw_signature: String::new(),
            throws: throws.iter().map(|s| s.to_string()).collect(),
            annotations: Vec::new(),
        }
    }

    fn ty(superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// `Object ← Throwable ← Exception ← {IOException ← FileNotFoundException, SQLException}` plus the
    /// unchecked branch. `Reader.read()` throws `IOException`; `Plain.calc()` throws nothing.
    fn resolver() -> MapResolver {
        let mut m = HashMap::new();
        m.insert("java/lang/Object".into(), ty(None, Vec::new()));
        m.insert("java/lang/Throwable".into(), ty(Some("java/lang/Object"), Vec::new()));
        m.insert("java/lang/Exception".into(), ty(Some("java/lang/Throwable"), Vec::new()));
        m.insert("java/io/IOException".into(), ty(Some("java/lang/Exception"), Vec::new()));
        m.insert(
            "java/io/FileNotFoundException".into(),
            ty(Some("java/io/IOException"), Vec::new()),
        );
        m.insert("java/sql/SQLException".into(), ty(Some("java/lang/Exception"), Vec::new()));
        m.insert(
            "java/lang/RuntimeException".into(),
            ty(Some("java/lang/Exception"), Vec::new()),
        );
        m.insert(
            "com/acme/Reader".into(),
            ty(Some("java/lang/Object"), vec![method("read", &["java/io/IOException"])]),
        );
        m.insert("com/acme/Plain".into(), ty(Some("java/lang/Object"), vec![method("calc", &[])]));
        m.insert(
            "java/lang/CloneNotSupportedException".into(),
            ty(Some("java/lang/Exception"), Vec::new()),
        );
        m.insert(
            "javax/crypto/Mac".into(),
            ty(
                Some("java/lang/Object"),
                vec![method("clone", &["java/lang/CloneNotSupportedException"])],
            ),
        );
        m.insert(
            "java/util/concurrent/TimeoutException".into(),
            ty(Some("java/lang/Exception"), Vec::new()),
        );
        // Two overloads of one name, and only the timed one declares `TimeoutException` — the shape
        // that made the intersection the wrong bound.
        m.insert(
            "java/util/concurrent/Future".into(),
            ty(
                Some("java/lang/Object"),
                vec![
                    method("get", &[]),
                    method("get", &["java/util/concurrent/TimeoutException"]),
                ],
            ),
        );
        let simple = [
            ("Object", "java/lang/Object"),
            ("Throwable", "java/lang/Throwable"),
            ("Exception", "java/lang/Exception"),
            ("IOException", "java/io/IOException"),
            ("FileNotFoundException", "java/io/FileNotFoundException"),
            ("SQLException", "java/sql/SQLException"),
            ("RuntimeException", "java/lang/RuntimeException"),
            ("Reader", "com/acme/Reader"),
            ("Plain", "com/acme/Plain"),
            ("Future", "java/util/concurrent/Future"),
            ("Mac", "javax/crypto/Mac"),
            ("CloneNotSupportedException", "java/lang/CloneNotSupportedException"),
            ("TimeoutException", "java/util/concurrent/TimeoutException"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        MapResolver { members: m, simple }
    }

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = bennu_java::prelude::extract_symbols_from_root(&root, src);
        let cache = InferCache::new();
        dead_catch_errors_in(root, &nodes, src, &symbols, &resolver(), &cache)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    /// What a refactor leaves behind: the call that threw it moved away.
    #[test]
    fn a_catch_for_something_the_body_cannot_throw_is_flagged() {
        let src = "class A { void go(Plain p) { try { p.calc(); } catch (SQLException e) {} } }";
        assert_eq!(codes(src), ["unthrown-catch"]);
    }

    #[test]
    fn a_catch_for_what_the_body_does_throw_is_fine() {
        let src = "class A { void go(Reader r) { try { r.read(); } catch (IOException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// Narrowing is legal: `FileNotFoundException` is a subtype of what `read()` declares.
    #[test]
    fn catching_a_subtype_of_what_is_thrown_is_legal() {
        let src = "class A { void go(Reader r) { try { r.read(); } catch (FileNotFoundException e) {} } }";
        assert!(codes(src).is_empty());
    }

    #[test]
    fn an_explicit_throw_in_the_body_counts() {
        let src = "class A { void go(Plain p) { try { throw new SQLException(); } catch (SQLException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// Always legal — they also cover the unchecked exceptions any statement can raise.
    #[test]
    fn exception_and_throwable_are_always_legal() {
        for t in ["Exception", "Throwable"] {
            let src = format!("class A {{ void go(Plain p) {{ try {{ p.calc(); }} catch ({t} e) {{}} }} }}");
            assert!(codes(&src).is_empty(), "{t}");
        }
    }

    #[test]
    fn an_unchecked_catch_type_is_legal_anywhere() {
        let src = "class A { void go(Plain p) { try { p.calc(); } catch (RuntimeException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// The load-bearing gate: one call we cannot resolve and the body could throw anything.
    #[test]
    fn one_unresolvable_call_abandons_the_whole_try() {
        let src = "class A { void go(Plain p, Mystery m) { try { p.calc(); m.whatever(); } catch (SQLException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// An inner `catch` can consume what the outer one is waiting for.
    #[test]
    fn a_nested_try_abandons_the_outer_one() {
        let src = "class A { void go(Plain p) { try { try { p.calc(); } catch (RuntimeException e) {} } catch (SQLException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// A lambda body runs later, or not at all — not inside this `try`.
    #[test]
    fn a_call_inside_a_lambda_does_not_count_as_thrown_here() {
        let src = "class A { void go(Reader r) { try { Runnable x = () -> { r.read(); }; } catch (IOException e) {} } }";
        assert_eq!(codes(src), ["unthrown-catch"]);
    }

    /// The multi-catch reports only the arm that cannot arrive.
    #[test]
    fn a_multi_catch_reports_only_the_dead_arm() {
        let src = "class A { void go(Reader r) { try { r.read(); } catch (IOException | SQLException e) {} } }";
        assert_eq!(codes(src), ["unthrown-catch"]);
    }

    /// The regression that guava found: `Future.get(timeout, unit)` declares `TimeoutException`
    /// while the no-arg `Future.get()` does not. Reading the INTERSECTION of the overloads — which
    /// is the right answer for "what must be handled" — drops it, and reports a `catch` the code has
    /// needed for a decade. The upper bound is the only sound basis for concluding "never".
    #[test]
    fn an_overload_that_declares_it_is_enough_even_if_a_sibling_does_not() {
        let src = "class A { void go(Future f) { try { f.get(1, null); } catch (TimeoutException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// The second half of the same lesson, also from guava: the `Object.clone()` carve-out in
    /// `throws_of` suppresses the LOWER bound so `array.clone()` is not reported as unhandled. It
    /// must not zero the upper one — `javax.crypto.Mac` overrides `clone()` and really does declare
    /// `CloneNotSupportedException`.
    #[test]
    fn a_receiver_whose_own_clone_declares_it_can_still_throw_it() {
        let src = "class A { void go(Mac m) { try { m.clone(); } catch (CloneNotSupportedException e) {} } }";
        assert!(codes(src).is_empty());
    }

    /// A `throw` of something that is not written down (`throw e;`) is not counted — we give up
    /// rather than under-count and invent a dead catch.
    #[test]
    fn a_rethrow_of_a_variable_abandons_the_try() {
        let src = "class A { void go(Plain p, SQLException boom) { try { throw boom; } catch (SQLException e) {} } }";
        assert!(codes(src).is_empty());
    }
}
