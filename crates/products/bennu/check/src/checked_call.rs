//! Resolver-backed **unreported exception from a call** diagnostic (`"error"`). A method or
//! constructor call that can throw a checked exception which is NEITHER caught by an enclosing `try`
//! NOR declared in the enclosing method/constructor's `throws` clause is a compile error —
//! `Files.readAllBytes(path)` (declares `throws IOException`) with no surrounding try and no
//! `throws IOException` on the caller → "unhandled checked exception".
//!
//! This is the CALL-SITE sibling of [`crate::checked_throw`]: it shares the exact same
//! caught/declared/checked machinery, but where `checked_throw` reads the thrown type off a literal
//! `throw new T(...)`, here the candidate thrown set comes from the CALLED member's `throws` list
//! (populated for JDK/library methods from bytecode's `Exceptions` attribute and for project methods
//! from the source `throws` clause — `bennu_java::prelude::Member::throws`).
//!
//! ## PARAMOUNT: never a false positive. This is exception flow → EXTRA conservative; unknown = SKIP.
//!
//! ### Overload SOUNDNESS (the crux)
//! A call `x.f(args)` may bind to any of several overloads of `f`. We do NOT do full overload
//! resolution (argument-type matching against every parameter, boxing, varargs, generics) — that is
//! exactly the machinery that, done imperfectly, would produce false positives. Instead we take the
//! **INTERSECTION** of the `throws` lists across ALL candidate members of that name (for a ctor, all
//! `<init>` of the resolved type). Rationale: an exception that EVERY candidate declares is thrown no
//! matter which overload the compiler actually picks — so reporting it is sound without knowing the
//! binding. An exception only SOME overloads declare might not be thrown by the one that binds, so it
//! is dropped by the intersection (never flagged). Zero candidates → nothing definite → SKIP.
//!
//! ### Per-exception gate (mirrors `checked_throw`'s fully-known requirement)
//! From the intersection we keep only entries `x` such that `is_checked(resolver, x)` AND
//! `hierarchy_fully_known(resolver, x)`. The fully-known gate is what makes `is_checked`'s NEGATIVE
//! facts ("does NOT reach RuntimeException/Error") trustworthy — an unknown link in `x`'s ancestry
//! could hide an unchecked base, so without this gate we might wrongly classify an unchecked type as
//! checked and flag it. A candidate carrying an exception with an unknown hierarchy → that exception
//! is simply dropped (not flagged), never the whole call skipped, so other definitely-thrown checked
//! types are still reported.
//!
//! For each surviving checked `x`: if NOT caught by an enclosing `try` AND NOT declared by the
//! enclosing callable's `throws` → one diagnostic per unhandled checked type, anchored on the call's
//! method name (or the `new` type node for a constructor).

use bennu_java::prelude::{FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::throws_of::{thrown_by, Thrown};

use crate::checked_throw::{
    callable_in_synthetic_type, callable_sneaky_throws, caught_by_enclosing_try,
    declared_by_callable, enclosing_callable, is_checked,
};
use crate::nodes::simple_name;
use crate::walk::hierarchy_fully_known;

/// One call that can throw a checked exception nothing handles — the analysis's own answer, before
/// it is turned into a message.
///
/// The diagnostic is a sentence; a **quick-fix** needs the parts of it. Which exception, so the fix
/// can name the type it adds; where the `throws` clause goes; and what a `try` would have to wrap.
/// Recomputing those from the diagnostic would mean parsing our own message back — and computing
/// them a second time somewhere else would mean two implementations of "is this handled?", which is
/// exactly the question that must have one answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnhandledCall {
    /// JVM binary name of the exception (`java/io/IOException`).
    pub exception: String,
    /// Byte span the diagnostic underlines — the call's name, or a constructor's type.
    pub anchor: (usize, usize),
    /// Byte offset where a `throws` clause would be inserted on the enclosing callable.
    pub throws_insert: usize,
    /// Byte span of the statement containing the call — what a `try { … }` would wrap.
    pub statement: (usize, usize),
}

/// The checked exceptions the code between `start` and `end` can raise, as JVM binary names.
///
/// The question *extract method* has and cannot answer for itself: what the moved body needs on its
/// `throws` clause. The refactoring crate reads the tree and nothing else, so its own answer is the
/// enclosing method's clause plus whatever the surrounding `try` catches — sound where those cover
/// it, and wrong wherever a call raises something the enclosing method neither declares nor catches
/// because a `try` INSIDE the selection handled it, or because the enclosing method is
/// `@SneakyThrows`.
///
/// A **lower bound**, and it has to be used as one: what this can prove, to be added to whatever the
/// caller already had. It is not a complete set and cannot be — a call it cannot resolve (a bare
/// call to a method of the same class is the ordinary case) simply contributes nothing, and there is
/// no honest way to tell "this throws nothing" from "I could not read this" per call without
/// throwing away the calls it *can* read.
///
/// Which settles the direction. Removing an exception from a `throws` clause on the strength of an
/// incomplete set breaks the call site; adding one that cannot actually be raised is legal and
/// costs nothing. So this only ever adds.
///
/// Both bounds of each call are included for the same reason: `possibly` is still a reason to
/// declare.
pub fn checked_exceptions_in(
    source: &str,
    start: usize,
    end: usize,
    resolver: &dyn TypeResolver,
) -> Vec<String> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let symbols = bennu_java::prelude::extract_symbols(source);
    let cache = InferCache::new();
    let mut out: Vec<String> = Vec::new();

    for n in crate::prelude::collect_nodes(root) {
        if n.start_byte() < start || n.end_byte() > end {
            continue;
        }
        let raised: Vec<String> = match n.kind() {
            "method_invocation" | "object_creation_expression" => {
                match thrown_by(n, &root, source, bytes, &symbols, resolver, &cache) {
                    Thrown::Known(_, throws) => {
                        throws.definitely.iter().chain(throws.possibly.iter()).cloned().collect()
                    }
                    // A call we cannot read contributes nothing. It does NOT make the answer
                    // useless, because the answer is only ever added to.
                    Thrown::Unknown => Vec::new(),
                }
            }
            // `throw new IOException()` raises it whatever any signature says.
            "throw_statement" => n
                .named_child(0)
                .filter(|e| e.kind() == "object_creation_expression")
                .and_then(|e| e.child_by_field_name("type"))
                .and_then(|t| t.utf8_text(bytes).ok())
                .and_then(|name| crate::resolve::type_binary(name, &symbols, resolver))
                .into_iter()
                .collect(),
            _ => continue,
        };
        for binary in raised {
            if out.contains(&binary) {
                continue;
            }
            if hierarchy_fully_known(resolver, &binary) && is_checked(resolver, &binary) {
                out.push(binary);
            }
        }
    }
    out.sort();
    out
}

/// Parse `source` and flag calls that can throw an unhandled checked exception.
pub fn checked_call_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    checked_call_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` + reuses `root` (for receiver inference) + `symbols`
/// + the shared per-file inference `cache`. Mirrors `members::unknown_members_in`.
pub fn checked_call_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    unhandled_calls_in(root, nodes, source, symbols, resolver, cache)
        .into_iter()
        .map(|u| {
            crate::check_id::CheckId::UnhandledCheckedException.span(
                u.anchor.0,
                u.anchor.1,
                format!(
                    "Unhandled exception: `{}` must be caught or declared to be thrown",
                    simple_name(&u.exception)
                ),
            )
        })
        .collect()
}

/// The analysis itself: every call that can throw a checked exception nothing handles.
///
/// [`checked_call_errors_in`] is this plus a sentence. A quick-fix wants the [`UnhandledCall`]s —
/// see that type for why they are not recovered from the diagnostic.
pub fn unhandled_calls_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<UnhandledCall> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // Both kinds ask the same question of `throws_of`, which owns the reading of a
            // `throws` clause; what differs is only what we do with the answer.
            "method_invocation" | "object_creation_expression" => {
                if let Thrown::Known(anchor, thrown) =
                    thrown_by(n, &root, source, bytes, symbols, resolver, cache)
                {
                    flag_unhandled(n, anchor, &thrown.definitely, bytes, symbols, resolver, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// A `receiver.method(args)` (or, when the receiver type can be inferred, a bare/`this` call is
/// deliberately NOT handled — see below). Collects the name-matching overloads across the receiver
/// type's fully-known hierarchy, intersects their `throws`, and flags each definitely-thrown checked
/// exception that is neither caught nor declared.
#[allow(clippy::too_many_arguments)]
/// For each definitely-thrown exception in `thrown`, keep only those that are provably CHECKED over a
/// FULLY-KNOWN hierarchy, then flag the ones neither caught by an enclosing `try` nor declared by the
/// enclosing callable's `throws`. `anchor` is where the diagnostic points (method name / `new` type).
#[allow(clippy::too_many_arguments)]
fn flag_unhandled(
    call: Node,
    anchor: Node,
    thrown: &[String],
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<UnhandledCall>,
) {
    if thrown.is_empty() {
        return;
    }

    // SKIP the whole call if there's no plain method/constructor enclosing it. The nearest boundary
    // could be a lambda / anonymous / local class whose throws-ability we don't model. `enclosing_
    // callable` returns the nearest boundary node; we handle it ONLY if it's a real method/ctor.
    let Some(callable) = enclosing_callable(call) else { return };
    if !matches!(callable.kind(), "method_declaration" | "constructor_declaration") {
        return;
    }
    // SKIP: even a real method/ctor, if it belongs to an anonymous/local class, has a non-authoritative
    // `throws` contract (SAM / capture machinery we don't resolve) → stay sound.
    if callable_in_synthetic_type(callable) {
        return;
    }
    // Lombok `@SneakyThrows` on the enclosing method/ctor lets it throw any checked exception without
    // declaring it → never flag a checked-throwing call inside it.
    if callable_sneaky_throws(callable, bytes) {
        return;
    }

    // De-dup: intersection order may repeat across identical overloads; report each type once.
    let mut seen: Vec<&str> = Vec::new();
    for binary in thrown {
        if seen.iter().any(|s| *s == binary.as_str()) {
            continue;
        }
        // Per-exception fully-known gate: without it `is_checked`'s "not RuntimeException/Error"
        // negatives aren't trustworthy (an unknown ancestor could be unchecked) → could false-positive.
        // A checked exception whose hierarchy has a gap is simply DROPPED (not flagged), never causing
        // us to skip the other definitely-thrown checked types.
        if !hierarchy_fully_known(resolver, binary) {
            continue;
        }
        // Keep only CHECKED exceptions — reaches Throwable but neither RuntimeException nor Error.
        if !is_checked(resolver, binary) {
            continue;
        }
        seen.push(binary.as_str());

        // Handled by an enclosing `try` (catch of this type or a supertype)? → SKIP this exception.
        if caught_by_enclosing_try(call, callable, bytes, symbols, resolver, binary) {
            continue;
        }
        // Declared by the enclosing method/ctor's `throws` (this type or a supertype)? → SKIP.
        if declared_by_callable(callable, bytes, symbols, resolver, binary) {
            continue;
        }

        // Survived every SKIP: a checked exception the call can throw, not caught, not declared.
        out.push(UnhandledCall {
            exception: binary.clone(),
            anchor: (anchor.start_byte(), anchor.end_byte()),
            throws_insert: throws_insertion_point(callable),
            statement: enclosing_statement(call)
                .map(|s| (s.start_byte(), s.end_byte()))
                .unwrap_or((call.start_byte(), call.end_byte())),
        });
    }
}

/// Where a `throws` clause would go on `callable`: just after its parameter list's `)`.
///
/// Not "before the body", because a constructor or an abstract method may have annotations, type
/// parameters or a `default` value between the two, and a clause spliced in there is a syntax error.
/// The `)` is the one landmark every callable has in the same place.
fn throws_insertion_point(callable: Node) -> usize {
    callable
        .child_by_field_name("parameters")
        .map(|p| p.end_byte())
        // A callable with no parameter list is not a shape we can edit; anchoring on its start makes
        // the caller's `throws` offer land somewhere visible rather than silently nowhere.
        .unwrap_or_else(|| callable.start_byte())
}

/// The statement containing `call` — what a `try { … }` would have to wrap.
///
/// The whole statement, not the call: wrapping `Files.readAllBytes(p)` alone inside
/// `byte[] b = try { … }` is not Java. Stops at the callable boundary, so a call in a field
/// initialiser (which is inside no statement) yields nothing rather than the whole class body.
fn enclosing_statement(call: Node) -> Option<Node> {
    let mut cur = Some(call);
    while let Some(n) = cur {
        if n.kind().ends_with("_statement") || n.kind() == "local_variable_declaration" {
            return Some(n);
        }
        if matches!(n.kind(), "method_declaration" | "constructor_declaration" | "class_body") {
            return None;
        }
        cur = n.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same mock-resolver shape as `members.rs` / `checked_throw.rs`: a `binary → members` map +
    /// a `simple → binary` table.
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

    /// A type body with the given superclass/interfaces + methods.
    fn cm(superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// A bare no-arg method with the given `throws`.
    fn m_throws(name: &str, throws: Vec<&str>) -> Member {
        Member::method(name, TypeRef::simple("void"), Vec::new())
            .throws(throws.into_iter().map(|s| s.to_string()).collect())
    }

    /// Exception hierarchy `Object ← Throwable ← Exception ← IOException` (checked), plus the unchecked
    /// branch `Exception ← RuntimeException ← IllegalStateException`. A `Files` type with a static
    /// `readAllBytes` that `throws IOException`, an `App` type whose ctor throws IOException, and a
    /// `Svc` type used for the overload/unchecked cases.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), cm(None, vec![]));
        members.insert("java/lang/Throwable".into(), cm(Some("java/lang/Object"), vec![]));
        members.insert("java/lang/Exception".into(), cm(Some("java/lang/Throwable"), vec![]));
        members.insert("java/io/IOException".into(), cm(Some("java/lang/Exception"), vec![]));
        members.insert(
            "java/lang/RuntimeException".into(),
            cm(Some("java/lang/Exception"), vec![]),
        );
        members.insert(
            "java/lang/IllegalStateException".into(),
            cm(Some("java/lang/RuntimeException"), vec![]),
        );

        // `Files.readAllBytes()` — static, throws checked IOException.
        members.insert(
            "acme/Files".into(),
            cm(
                Some("java/lang/Object"),
                vec![m_throws("readAllBytes", vec!["java/io/IOException"]).stat()],
            ),
        );
        // `App()` constructor throws checked IOException.
        members.insert(
            "acme/App".into(),
            cm(
                Some("java/lang/Object"),
                vec![m_throws("<init>", vec!["java/io/IOException"])],
            ),
        );
        // `Runtimey.boom()` throws an UNCHECKED RuntimeException subclass.
        members.insert(
            "acme/Runtimey".into(),
            cm(
                Some("java/lang/Object"),
                vec![m_throws("boom", vec!["java/lang/IllegalStateException"])],
            ),
        );
        // `Over` has TWO overloads of `maybe`: only ONE declares IOException → intersection empty.
        members.insert(
            "acme/Over".into(),
            cm(
                Some("java/lang/Object"),
                vec![
                    m_throws("maybe", vec!["java/io/IOException"]),
                    m_throws("maybe", vec![]),
                ],
            ),
        );

        let simple = [
            ("Object", "java/lang/Object"),
            ("Throwable", "java/lang/Throwable"),
            ("Exception", "java/lang/Exception"),
            ("IOException", "java/io/IOException"),
            ("RuntimeException", "java/lang/RuntimeException"),
            ("IllegalStateException", "java/lang/IllegalStateException"),
            ("Files", "acme/Files"),
            ("App", "acme/App"),
            ("Runtimey", "acme/Runtimey"),
            ("Over", "acme/Over"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags(src: &str) -> Vec<String> {
        checked_call_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    // ── positives ─────────────────────────────────────────────────────────────────

    #[test]
    fn checked_throwing_call_undeclared_is_flagged() {
        // `f.readAllBytes()` throws IOException; the method neither catches nor declares it → flagged.
        let d = diags("class C { void m() { Files f = null; f.readAllBytes(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(
            d[0].contains("Unhandled exception") && d[0].contains("IOException"),
            "{d:?}"
        );
    }

    #[test]
    fn checked_throwing_constructor_undeclared_is_flagged() {
        // `new App()` — its ctor throws IOException, not handled → flagged.
        let d = diags("class C { void m() { App a = new App(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("IOException"), "{d:?}");
    }

    // ── negatives ─────────────────────────────────────────────────────────────────

    #[test]
    fn caught_by_try_is_ok() {
        assert!(diags(
            "class C { void m() { Files f = null; try { f.readAllBytes(); } catch (IOException e) {} } }"
        )
        .is_empty());
    }

    #[test]
    fn declared_by_caller_is_ok() {
        assert!(diags(
            "class C { void m() throws IOException { Files f = null; f.readAllBytes(); } }"
        )
        .is_empty());
    }

    #[test]
    fn declared_supertype_by_caller_is_ok() {
        // `throws Exception` covers the checked IOException (subtype).
        assert!(diags(
            "class C { void m() throws Exception { Files f = null; f.readAllBytes(); } }"
        )
        .is_empty());
    }

    #[test]
    fn unchecked_throwing_call_is_ok() {
        // `boom()` throws IllegalStateException (unchecked) → never flagged.
        assert!(diags("class C { void m() { Runtimey r = null; r.boom(); } }").is_empty());
    }

    #[test]
    fn unresolvable_receiver_is_ok() {
        // `Mystery` doesn't resolve → no candidate set → SKIP.
        assert!(diags("class C { void m() { Mystery x = null; x.whatever(); } }").is_empty());
    }

    #[test]
    fn call_inside_lambda_is_ok() {
        // Nearest enclosing callable is a lambda → SKIP (its throws-ability is the SAM's, not modeled).
        assert!(diags(
            "class C { void m() { Runnable r = () -> { Files f = null; f.readAllBytes(); }; } }"
        )
        .is_empty());
    }

    #[test]
    fn overload_only_one_declares_checked_is_ok() {
        // `Over.maybe` has two overloads; only one declares IOException → intersection empty → SKIP.
        assert!(diags("class C { void m() { Over o = null; o.maybe(); } }").is_empty());
    }

    #[test]
    fn bare_call_is_skipped() {
        // A bare `readAllBytes()` (no explicit receiver) is not checked → silent.
        assert!(diags("class C { void m() { readAllBytes(); } }").is_empty());
    }

    #[test]
    fn checked_call_in_resource_spec_caught_by_try_is_ok() {
        // A checked-throwing call in the RESOURCE initializer of a try-with-resources is protected by
        // that try's catch (JLS §14.20.3) — here via a multi-catch alternative. Was falsely flagged
        // because the resource specification wasn't treated as the try's protected region.
        let src = "class C { void m() { Files f = null; try (Object o = f.readAllBytes()) {} catch (IOException | RuntimeException e) {} } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn sneaky_throws_suppresses_a_checked_call() {
        // Lombok `@SneakyThrows` lets the method call a checked-throwing method without handling it.
        assert!(
            diags("class C { @SneakyThrows void m() { Files f = null; f.readAllBytes(); } }").is_empty(),
            "@SneakyThrows must suppress the unhandled-checked-call flag"
        );
    }

    #[test]
    fn checked_call_in_resource_spec_uncaught_is_flagged() {
        // The same resource-initializer call, but no catch covers IOException → still flagged (the
        // resource-spec handling protects, it doesn't blanket-suppress).
        let src = "class C { void m() { Files f = null; try (Object o = f.readAllBytes()) {} catch (RuntimeException e) {} } }";
        let d = diags(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("IOException"), "{d:?}");
    }
}
