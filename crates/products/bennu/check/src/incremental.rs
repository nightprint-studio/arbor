//! Out-of-code-block **incremental** validation — re-validate only the method / constructor bodies
//! whose text changed, reusing cached diagnostics for the unchanged ones. This is what keeps a big
//! legacy class cheap to re-check while you type inside one method: the expensive resolver-backed
//! checks (type inference on every call / field access / cast) run only over the body you touched,
//! not the whole file.
//!
//! ## Correctness (cardinal rule: never a wrong verdict)
//!
//! The result is a **byte-for-byte-equivalent multiset** to a full [`check_file_resolved`] run — the
//! reuse only skips *recomputation*, never changes *what is reported*. Three things make that sound:
//!
//! 1. **Cheap checks always run fresh over the whole file.** The pure-AST checks and the
//!    declaration-oriented resolver checks (imports, unresolved types, inheritance, overrides, cycles,
//!    definite-final, static-context, …) are recomputed every time, so their diagnostics are always
//!    current and correctly positioned. Only the *expensive per-expression* checks are cached.
//!
//! 2. **The expensive checks run over a true PARTITION of the nodes.** Every node lands in exactly one
//!    bucket — a *top* method / constructor body (one not nested inside another method body; a local /
//!    anonymous class inside a body is folded into that body), or the *structural remainder*
//!    (everything outside those bodies: field initializers, initializer blocks, signatures, headers).
//!    The union of buckets is the whole file, and each per-expression check visits a node the same way
//!    whether it iterates the whole slice or one bucket, so the union of the buckets' results equals a
//!    whole-file run. The structural remainder is always recomputed (it's small, and it keeps field
//!    initializers / `static {}` blocks covered).
//!
//! 3. **A body's cache is reused only when its inputs are unchanged.** A body's expensive diagnostics
//!    depend on (its own body text, the file's STRUCTURE — imports / fields / every signature /
//!    `extends` / `implements`, captured by `structural_hash` — and the project resolver, captured by
//!    `resolver_rev`), and never on another body's *content*. So a body's cached diagnostics (stored
//!    relative to the body start, rebased on reuse) are valid iff `structural_hash`, `resolver_rev`
//!    and the body's own text hash all match. Any mismatch → recompute. When the structure or the
//!    resolver changed, every body is recomputed.
//!
//! The `check_file_resolved_incremental` equivalence tests below assert exactly this — that a fresh
//! incremental run, and every run after an edit (body-only, signature, field-initializer, added
//! method, nested local class), reproduces the full run's diagnostics.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use bennu_java::prelude::{extract_symbols_from_root, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check::{check_file, check_file_in, collect_nodes, finish, FileContext};

/// Per-file state threaded across successive validations of the same buffer. Opaque to callers — hand
/// the same `&mut` back each time; a `Default` (empty) cache just means "first run, recompute all".
#[derive(Default)]
pub struct IncrementalCache {
    /// Hash of the file with every top body's interior blanked — captures imports, package,
    /// fields+initializers, all signatures, `extends`/`implements`, modifiers. A change here means a
    /// structural edit → every body is recomputed.
    structural_hash: u64,
    /// The resolver revision the cache was built against (project index generation + cross-file
    /// overlay edits). A change here means a type the bodies resolve against may have moved → recompute.
    resolver_rev: u64,
    /// One entry per top method / constructor body, in document order.
    bodies: Vec<BodyEntry>,
}

struct BodyEntry {
    /// Hash of the body's source bytes.
    text_hash: u64,
    /// The body's expensive-check diagnostics, offsets stored RELATIVE to the body's start byte (so
    /// they survive the body shifting when an earlier body changes length).
    rel_diags: Vec<Diagnostic>,
}

/// Like [`check_file_resolved`](crate::check::check_file_resolved), but reuses cached diagnostics for
/// unchanged method / constructor bodies. `resolver_rev` is an opaque revision the caller bumps
/// whenever the resolver's answers could change (project re-index, or another file's buffer edited);
/// `cache` is the per-file state (pass `&mut IncrementalCache::default()` for a one-shot / first run).
///
/// The returned diagnostics are the same multiset a full run would produce (see the module docs).
pub fn check_file_resolved_incremental(
    source: &str,
    ctx: &FileContext,
    resolver: &dyn TypeResolver,
    jdk_available: bool,
    resolver_rev: u64,
    cache: &mut IncrementalCache,
) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        *cache = IncrementalCache::default();
        return check_file(source, ctx);
    };
    let root = tree.root_node();
    let nodes = collect_nodes(root);

    // Pure-AST checks: always fresh (cheap, and their offsets must be current).
    let mut out = check_file_in(root, &nodes, source, ctx);

    // A file that did not PARSE gets its syntax error and nothing else — same rule, and the same
    // reason, as the whole-file path (`check::check_file_resolved`). Drop the cache too: entries
    // keyed to a tree we do not believe would be served back once the file parses again.
    if root.has_error() {
        *cache = IncrementalCache::default();
        return finish(out);
    }

    // No JDK → the resolver checks would all resolve to "unknown" and stay silent; skip them (and drop
    // any stale cache, so a later JDK-available run rebuilds from scratch).
    if !jdk_available {
        *cache = IncrementalCache::default();
        return finish(out);
    }

    let symbols = extract_symbols_from_root(&root, source);

    // Whole-file resolver checks: declaration-oriented / file-gated → always fresh.
    out.extend(run_wholefile_checks(root, &nodes, source, &symbols, resolver, ctx));

    // Partition by top method / constructor body and (re)compute the expensive per-expression checks.
    let bytes = source.as_bytes();
    let top_bodies = top_body_ranges(&nodes);
    let structural = structural_hash(bytes, &top_bodies);
    let infer = InferCache::new();

    // The cache is reusable only when the structure, the resolver and the body count all match; a
    // count mismatch alone would desync the by-ordinal mapping, so it forces a full rebuild.
    let reusable = cache.structural_hash == structural
        && cache.resolver_rev == resolver_rev
        && cache.bodies.len() == top_bodies.len();

    let mut new_bodies: Vec<BodyEntry> = Vec::with_capacity(top_bodies.len());
    for (i, &(bs, be)) in top_bodies.iter().enumerate() {
        let text_hash = hash_bytes(&bytes[bs..be]);
        if reusable && cache.bodies[i].text_hash == text_hash {
            // Unchanged body → replay its cached diagnostics, rebased to the current body start.
            for d in &cache.bodies[i].rel_diags {
                out.push(Diagnostic {
                    message: d.message.clone(),
                    severity: d.severity.clone(),
                    code: d.code.clone(),
                    start: d.start + bs,
                    end: d.end + bs,
                });
            }
            new_bodies.push(BodyEntry { text_hash, rel_diags: cache.bodies[i].rel_diags.clone() });
        } else {
            // New / changed body → run the per-expression checks over just its nodes; cache them
            // relative to the body start so a later shift replays them at the right place.
            let body_nodes = nodes_in_range(&nodes, bs, be);
            let diags = run_body_checks(root, &body_nodes, source, &symbols, resolver, &infer);
            let rel_diags = diags
                .iter()
                .map(|d| Diagnostic {
                    message: d.message.clone(),
                    severity: d.severity.clone(),
                    code: d.code.clone(),
                    start: d.start.saturating_sub(bs),
                    end: d.end.saturating_sub(bs),
                })
                .collect();
            out.extend(diags);
            new_bodies.push(BodyEntry { text_hash, rel_diags });
        }
    }

    // The structural remainder (field initializers, initializer blocks, anything outside a top body)
    // is always recomputed fresh — small, and it keeps those expressions covered.
    let outside = nodes_outside_ranges(&nodes, &top_bodies);
    out.extend(run_body_checks(root, &outside, source, &symbols, resolver, &infer));

    *cache = IncrementalCache { structural_hash: structural, resolver_rev, bodies: new_bodies };

    finish(out)
}

/// The expensive, per-expression resolver checks — run over a node bucket (a body, or the structural
/// remainder). These are exactly the checks [`check_file_resolved`](crate::check::check_file_resolved)
/// runs with the shared [`InferCache`]; keep the two lists in lock-step.
fn run_body_checks<'a>(
    root: Node<'a>,
    nodes: &[Node<'a>],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    infer: &InferCache,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    out.extend(crate::members::unknown_members_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::fields::unknown_fields_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::arity::arity_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::arguments::argument_type_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::casts::type_compat_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::enum_switch::enum_switch_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::switch_label_type::switch_label_type_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::super_method::super_method_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::condition_type::condition_type_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::type_use::type_use_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::narrowing::narrowing_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::checked_call::checked_call_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::visibility::visibility_errors_in(root, nodes, source, symbols, resolver, infer));
    out.extend(crate::checked_throw::checked_throw_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::exceptions::exception_errors_in(nodes, source, symbols, resolver));
    out
}

/// The whole-file resolver checks — declaration-oriented or file-gated, always recomputed. These are
/// the resolver checks [`check_file_resolved`](crate::check::check_file_resolved) runs WITHOUT the
/// shared [`InferCache`]; keep the two lists in lock-step.
fn run_wholefile_checks<'a>(
    root: Node<'a>,
    nodes: &[Node<'a>],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    ctx: &FileContext,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    out.extend(crate::imports::unresolved_imports(root, source, resolver, ctx.classpath_complete));
    out.extend(crate::imports::unresolved_static_imports(root, source, resolver));
    out.extend(crate::types::unresolved_types_in(nodes, source, symbols, resolver));
    out.extend(crate::type_arg_arity::type_arg_arity_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::undefined_var::undefined_var_errors_in(root, nodes, source, symbols, resolver));
    out.extend(crate::unresolved_call::unresolved_call_errors_in(root, nodes, source, symbols, resolver));
    out.extend(crate::inheritance::inheritance_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::inheritance::missing_abstract_impls_in(nodes, source, symbols, resolver));
    out.extend(crate::functional::functional_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::constructors::super_constructor_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::finals::final_override_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::override_return::override_return_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::inherit_cycle::inherit_cycle_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::throws_widen::throws_widen_errors_in(nodes, source, symbols, resolver));
    out.extend(crate::static_access::static_access_errors_in(root, nodes, source, symbols, resolver));
    out
}

/// The byte ranges of the *top* method / constructor bodies — a body block not contained in another
/// body block (so a local / anonymous class's method inside a body is folded into the enclosing top
/// body, never a bucket of its own). Sorted by start, pairwise-disjoint.
fn top_body_ranges(nodes: &[Node]) -> Vec<(usize, usize)> {
    let mut all: Vec<(usize, usize)> = Vec::new();
    for &n in nodes {
        if matches!(
            n.kind(),
            "method_declaration" | "constructor_declaration" | "compact_constructor_declaration"
        ) {
            if let Some(body) = n.child_by_field_name("body") {
                all.push((body.start_byte(), body.end_byte()));
            }
        }
    }
    let mut top: Vec<(usize, usize)> = all
        .iter()
        .copied()
        .filter(|&(bs, be)| {
            // Keep it unless some OTHER body strictly contains it.
            !all.iter().any(|&(os, oe)| (os, oe) != (bs, be) && os <= bs && be <= oe)
        })
        .collect();
    top.sort_by_key(|&(bs, _)| bs);
    top.dedup();
    top
}

/// Nodes fully inside `[bs, be)` — a body bucket.
fn nodes_in_range<'a>(nodes: &[Node<'a>], bs: usize, be: usize) -> Vec<Node<'a>> {
    nodes
        .iter()
        .copied()
        .filter(|n| n.start_byte() >= bs && n.end_byte() <= be)
        .collect()
}

/// Nodes NOT inside any body range — the structural remainder.
fn nodes_outside_ranges<'a>(nodes: &[Node<'a>], bodies: &[(usize, usize)]) -> Vec<Node<'a>> {
    nodes
        .iter()
        .copied()
        .filter(|n| {
            let (s, e) = (n.start_byte(), n.end_byte());
            !bodies.iter().any(|&(bs, be)| s >= bs && e <= be)
        })
        .collect()
}

fn hash_bytes(b: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(b);
    h.finish()
}

/// Hash the file with every top body's bytes replaced by a fixed `{}` placeholder, so a body-content
/// edit leaves it unchanged while a signature / field / import edit changes it. `top_bodies` must be
/// sorted by start and disjoint; an out-of-order range is skipped defensively (kept panic-free).
fn structural_hash(bytes: &[u8], top_bodies: &[(usize, usize)]) -> u64 {
    let mut h = DefaultHasher::new();
    let mut pos = 0usize;
    for &(bs, be) in top_bodies {
        if bs < pos || be > bytes.len() || bs > be {
            continue;
        }
        h.write(&bytes[pos..bs]);
        h.write(b"{}");
        pos = be;
    }
    if pos <= bytes.len() {
        h.write(&bytes[pos..]);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::check_file_resolved;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    // ── A fixed mock resolver (same shape as the other checks' tests) ──────────────────────────────
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
    fn method(name: &str, ret: &str) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new()).sig(format!("{ret} {name}()"))
    }
    fn cm(superc: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superc.map(TypeRef::simple),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: Default::default(),
        }
    }
    /// `Object` (toString), `String` (length), and a project `com/acme/Foo` with `bar()` — enough to
    /// make `unknown_members` fire inside a body (`x.nope()` on a `String`, `f.nope()` on a `Foo`).
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, vec![method("toString", "java/lang/String")]));
        members.insert(
            "java/lang/String".to_string(),
            cm(Some("java/lang/Object"), vec![method("length", "int"), method("trim", "java/lang/String")]),
        );
        members.insert(
            "com/acme/Foo".to_string(),
            cm(Some("java/lang/Object"), vec![method("bar", "java/lang/String")]),
        );
        let mut simple = HashMap::new();
        simple.insert("String".to_string(), "java/lang/String".to_string());
        simple.insert("Foo".to_string(), "com/acme/Foo".to_string());
        simple.insert("Object".to_string(), "java/lang/Object".to_string());
        MapResolver { members, simple }
    }

    fn ctx() -> FileContext {
        FileContext::default()
    }

    /// A total, order-independent key for a diagnostic, so we compare the incremental run and the full
    /// run as MULTISETS (their same-position diagnostics can be emitted in a different order).
    fn norm(mut v: Vec<Diagnostic>) -> Vec<(usize, usize, String, String)> {
        let mut keyed: Vec<(usize, usize, String, String)> =
            v.drain(..).map(|d| (d.start, d.end, d.severity, d.message)).collect();
        keyed.sort();
        keyed
    }

    fn full(src: &str, r: &MapResolver) -> Vec<(usize, usize, String, String)> {
        norm(check_file_resolved(src, &ctx(), r, true))
    }
    fn incr(src: &str, r: &MapResolver, cache: &mut IncrementalCache, rev: u64) -> Vec<(usize, usize, String, String)> {
        norm(check_file_resolved_incremental(src, &ctx(), r, true, rev, cache))
    }

    /// The core invariant: a FRESH incremental run reproduces the full run, for every shape — this is
    /// what proves the node PARTITION doesn't drop or duplicate any expensive-check diagnostic.
    #[test]
    fn fresh_incremental_equals_full_over_many_shapes() {
        let r = resolver();
        let sources = [
            // two methods, one with a good call and one with a bad member on a String
            "package com.acme;\nclass A {\n  String a(String s) { return s.trim(); }\n  int b(String s) { return s.nope(); }\n}\n",
            // a project-type receiver with a bad member
            "package com.acme;\nclass A {\n  void m(Foo f) { f.bar(); f.nope(); }\n}\n",
            // a constructor body + a field initializer (structural remainder) with a bad member
            "package com.acme;\nclass A {\n  int n = \"x\".nope();\n  A(String s) { s.length(); s.gone(); }\n}\n",
            // a local class inside a method body (its call must still be checked, folded into the body)
            "package com.acme;\nclass A {\n  void m(String s) { class L { void k(Foo f) { f.nope(); } } s.length(); }\n}\n",
            // a lambda inside a field initializer (structural), with a bad member inside the lambda
            "package com.acme;\nclass A {\n  Runnable r = () -> { \"y\".gone(); };\n  void m() {}\n}\n",
            // try / switch / throw so the try/switch/throw body checks fire too
            "package com.acme;\nclass A {\n  void m(String s) { try { s.nope(); } catch (Exception e) {} }\n}\n",
            // no methods at all (all structural)
            "package com.acme;\nclass A {\n  String f = \"z\".missing();\n}\n",
        ];
        for src in sources {
            let mut cache = IncrementalCache::default();
            assert_eq!(incr(src, &r, &mut cache, 1), full(src, &r), "fresh incremental != full for:\n{src}");
        }
    }

    /// After a BODY-ONLY edit (signature unchanged), the reused-plus-recomputed result still equals a
    /// full run — and the earlier body's cached diagnostics are correctly rebased past the length change.
    #[test]
    fn body_edit_reuses_and_stays_equivalent() {
        let r = resolver();
        let v1 = "package com.acme;\nclass A {\n  int a(String s) { return s.nope(); }\n  int b(String s) { return s.gone(); }\n}\n";
        // Edit ONLY the first body (different bad member + different length), signatures identical.
        let v2 = "package com.acme;\nclass A {\n  int a(String s) { return s.absolutelyMissing(); }\n  int b(String s) { return s.gone(); }\n}\n";
        let mut cache = IncrementalCache::default();
        assert_eq!(incr(v1, &r, &mut cache, 1), full(v1, &r));
        // Reuse the cache from v1 → the second body is replayed, the first recomputed; must equal full.
        assert_eq!(incr(v2, &r, &mut cache, 1), full(v2, &r), "body-edit incremental != full");
    }

    /// A SIGNATURE edit changes the structural hash → every body recomputes; still equals full.
    #[test]
    fn signature_edit_rebuilds_and_stays_equivalent() {
        let r = resolver();
        let v1 = "package com.acme;\nclass A {\n  int a(String s) { return s.nope(); }\n}\n";
        let v2 = "package com.acme;\nclass A {\n  int a(String s, int extra) { return s.nope(); }\n}\n";
        let mut cache = IncrementalCache::default();
        assert_eq!(incr(v1, &r, &mut cache, 1), full(v1, &r));
        assert_eq!(incr(v2, &r, &mut cache, 1), full(v2, &r), "signature-edit incremental != full");
    }

    /// A bumped `resolver_rev` (project re-index / another file edited) invalidates the body cache even
    /// when the text is identical — the result is still a correct full run.
    #[test]
    fn resolver_rev_bump_invalidates_but_stays_equivalent() {
        let r = resolver();
        let src = "package com.acme;\nclass A {\n  void m(Foo f) { f.bar(); f.nope(); }\n}\n";
        let mut cache = IncrementalCache::default();
        assert_eq!(incr(src, &r, &mut cache, 1), full(src, &r));
        // Same source, new revision → recompute; identical result.
        assert_eq!(incr(src, &r, &mut cache, 2), full(src, &r), "rev-bump incremental != full");
    }

    /// Adding a method changes the body COUNT → the by-ordinal cache is rebuilt wholesale; equals full.
    #[test]
    fn added_method_rebuilds_and_stays_equivalent() {
        let r = resolver();
        let v1 = "package com.acme;\nclass A {\n  int a(String s) { return s.nope(); }\n}\n";
        let v2 = "package com.acme;\nclass A {\n  int a(String s) { return s.nope(); }\n  int c(String s) { return s.gone(); }\n}\n";
        let mut cache = IncrementalCache::default();
        assert_eq!(incr(v1, &r, &mut cache, 1), full(v1, &r));
        assert_eq!(incr(v2, &r, &mut cache, 1), full(v2, &r), "added-method incremental != full");
    }

    /// No JDK → only the pure-AST checks, and the cache is cleared. (A syntactically broken buffer must
    /// still be safe.)
    #[test]
    fn no_jdk_matches_pure_ast_and_clears_cache() {
        let r = resolver();
        let src = "package com.acme;\nclass A {\n  int a(String s) { return s.nope(); }\n}\n";
        let mut cache = IncrementalCache::default();
        let got = norm(check_file_resolved_incremental(src, &ctx(), &r, false, 1, &mut cache));
        let want = norm(check_file(src, &ctx()));
        assert_eq!(got, want);
        assert!(cache.bodies.is_empty());
    }
}
