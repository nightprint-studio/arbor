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

use bennu_java::prelude::{
    infer_node_type_cached, FileSymbols, InferCache, MemberKind, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::checked_throw::{
    callable_in_synthetic_type, caught_by_enclosing_try, declared_by_callable, enclosing_callable,
    is_checked,
};
use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::hierarchy_fully_known;

/// Parse `source` and flag calls that can throw an unhandled checked exception.
pub fn checked_call_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
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
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_invocation" => {
                check_invocation(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            "object_creation_expression" => {
                check_creation(n, bytes, symbols, resolver, &mut out)
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
fn check_invocation(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(name) = n.child_by_field_name("name") else { return };
    if name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };

    // SKIP: only an explicit-receiver call `obj.method(...)`. A bare `foo()` / implicit-`this` call
    // resolves against the enclosing type, whose source form we may not carry `throws` for reliably;
    // inferring it risks a false positive, so we stay silent (aligns with members/arity, which also
    // require an `object` field).
    let Some(obj) = n.child_by_field_name("object") else { return };
    // SKIP: receiver type not inferable, or inferred to the empty/unknown type → we can't gather a
    // trustworthy candidate set (an un-indexed type might declare/overload the method differently).
    let Some(ty) = infer_node_type_cached(root, source, symbols, &obj, resolver, cache) else {
        return;
    };
    if ty.binary_name.is_empty() {
        return;
    }

    // The overload set for this name across the receiver's hierarchy (memoized walk shared with the
    // member/arity/argument checks). `complete` is the hierarchy-fully-known gate.
    let res = cache.resolve_methods(resolver, &ty.binary_name, method);
    // SKIP: an unknown supertype might carry an overload with a DIFFERENT (smaller) `throws` list,
    // which would shrink the true intersection — so a hidden overload could make our intersection an
    // over-estimate → a false positive. Only a fully-known hierarchy makes the intersection sound.
    if !res.complete {
        return;
    }
    // SKIP: no candidate of that name (a missing method is `members.rs`'s job; here nothing definite
    // is thrown). Intersection over an empty set is meaningless → SKIP.
    if res.candidates.is_empty() {
        return;
    }

    let thrown = intersected_throws(&res.candidates);
    flag_unhandled(n, name, &thrown, bytes, symbols, resolver, out);
}

/// A `new T(args)` construction. Resolves `T`, gathers its OWN `<init>` members (constructors are not
/// inherited — mirror `arity::check_new`), intersects their `throws`, and flags unhandled checked
/// exceptions anchored on the `new`'s type node.
fn check_creation(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty_node) = n.child_by_field_name("type") else { return };

    // SKIP: an anonymous class `new Runnable(){…}` — the args bind to the supertype's constructor and
    // the anonymous body's own methods complicate the contract; stay out of it (mirror arity/members).
    // GOTCHA: explicit `for` loop over children (never `.any()` on `named_children`).
    let mut cw = n.walk();
    for c in n.named_children(&mut cw) {
        if c.kind() == "class_body" {
            return;
        }
    }

    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    // SKIP: `T` unresolvable → we don't know which constructors exist.
    let Some(binary) = type_binary(type_text, symbols, resolver) else { return };

    // Constructors are NOT inherited — look only at this class's own `<init>` methods (mirror arity).
    let Some(cm) = resolver.members_of(&binary) else { return };
    let ctors: Vec<&bennu_java::prelude::Member> = {
        let mut v = Vec::new();
        for m in &cm.methods {
            if m.name == "<init>" && m.kind == MemberKind::Method {
                v.push(m);
            }
        }
        v
    };
    // SKIP: no constructors indexed (the index may omit them) → nothing definite → SKIP.
    if ctors.is_empty() {
        return;
    }

    let thrown = intersected_throws_refs(&ctors);
    // Anchor the diagnostic on the `new`'s type node (there's no `name` field on a construction).
    flag_unhandled(n, ty_node, &thrown, bytes, symbols, resolver, out);
}

/// The INTERSECTION of `throws` across every candidate (owned `Member`s). See module docs: an
/// exception every overload declares is thrown regardless of which one binds → sound to report; one
/// only some declare is dropped. Empty when candidates disagree (or any declares nothing).
fn intersected_throws(candidates: &[bennu_java::prelude::Member]) -> Vec<String> {
    let refs: Vec<&bennu_java::prelude::Member> = candidates.iter().collect();
    intersected_throws_refs(&refs)
}

/// The intersection over borrowed candidates (constructors are gathered as refs). Start from the first
/// candidate's `throws` and retain only entries present in EVERY other candidate's `throws`. A single
/// candidate with an empty `throws` collapses the intersection to empty (correct: if one overload
/// throws nothing checked, we can't assume any checked exception is definitely thrown).
fn intersected_throws_refs(candidates: &[&bennu_java::prelude::Member]) -> Vec<String> {
    let Some((first, rest)) = candidates.split_first() else { return Vec::new() };
    let mut acc: Vec<String> = first.throws.clone();
    for cand in rest {
        // Keep only exceptions also declared by `cand` (set intersection; `throws` lists are tiny, so
        // a linear `contains` is fine and avoids allocating a HashSet per candidate).
        acc.retain(|x| cand.throws.iter().any(|y| y == x));
        if acc.is_empty() {
            break;
        }
    }
    acc
}

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
    out: &mut Vec<Diagnostic>,
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

        // Survived every SKIP: a checked exception the call can throw, not caught, not declared → error.
        out.push(Diagnostic {
            message: format!(
                "Unhandled exception: `{}` must be caught or declared to be thrown",
                simple_name(binary)
            ),
            severity: "error".to_string(),
            code: String::new(),
            start: anchor.start_byte(),
            end: anchor.end_byte(),
        });
    }
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
}
