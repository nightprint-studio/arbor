//! Unresolved-call diagnostics — a **bare method invocation** `foo()` whose name binds to nothing:
//! javac's `cannot find symbol: method foo()`. The companion of [`crate::undefined_var`], which asks
//! the same question about a bare *value* name.
//!
//! ## The gap this fills
//!
//! Worth naming, because each of the three checks around it was individually right to look away and
//! the hole was only visible from outside all three:
//!
//!   * [`crate::members`] (`unknown-member`) needs a receiver — it infers `x`'s type in `x.foo()` and
//!     asks that type for `foo`. A bare `foo()` has no receiver to infer, so it is skipped by
//!     construction (`let Some(obj) = … else { return }`), and says so in its own docs.
//!   * [`crate::undefined_var`] (`unresolved-symbol`) judges bare *identifiers*, and explicitly
//!     excludes a method-invocation's name: `foo` in `foo()` is not a variable reference, and
//!     treating it as one would be wrong.
//!   * [`crate::static_access`] does walk bare calls, but only to ask whether the name is a known
//!     INSTANCE member reached from a static context. A name that is no member at all falls out of
//!     its `if !is_instance` guard in silence.
//!
//! So a call whose static import was never written — `options()` with
//! `import static …WireMockConfiguration.options;` missing — resolved to nothing, matched no check,
//! and drew no squiggle. It is also the one shape the import checks structurally cannot catch:
//! [`crate::imports::unresolved_static_imports`] adjudicates the import lines that ARE written, and
//! here the defect is the line that isn't.
//!
//! ## Soundness — never a false positive
//!
//! Same contract as [`crate::undefined_var`], and for the same reason: this runs continuously while
//! someone types, and one wrong red squiggle costs more trust than ten missed real ones. Every doubt
//! SKIPs.
//!
//! WHOLE-FILE guards (any → produce NOTHING for the file):
//!   * a parse error anywhere (`has_error` on the root) — a broken buffer re-shapes the CST, and a
//!     qualified call can come out looking bare;
//!   * no single top-level class/enum, or its hierarchy not FULLY known — an un-indexed base class
//!     could declare the method;
//!   * an `import static X.*;` whose owner `X` is un-indexed — it could supply ANY bare name;
//!   * a **member-generating annotation** on the top type ([`crate::nodes::has_generated_members`]) —
//!     under Lombok's `@Data` the legal bare call `getName()` is declared nowhere in the source.
//!
//! PER-SITE guards (any failing → SKIP that call):
//!   * it must be a `method_invocation` with NO `object` field — `x.foo()`, `Type.foo()`,
//!     `super.foo()` all belong to [`crate::members`] / [`crate::super_method`], not here;
//!   * its enclosing type must be the top-level one, crossing no lambda and no nested / anonymous /
//!     local class body ([`crate::scopes::scope_is_directly_top`]) — any of those can declare methods
//!     we did not gather.
//!
//! RESOLUTION — flagged only when the name matches NONE of these:
//!   1. a method of the top type or any FULLY-KNOWN supertype (including the interfaces' defaults);
//!   2. a method **declared anywhere in this file** — read straight from the CST, so a method typed
//!      one second ago and not yet indexed is never reported as missing. This is the guard that keeps
//!      the check usable in an editor rather than only in a batch run;
//!   3. a name supplied by an `import static …` — a specific member, or any member of a fully-known
//!      wildcard owner;
//!   4. a method of `java.lang.Object` (`toString`, `equals`, `wait`, …), legal bare in every class
//!      whether or not the index bothered to list them;
//!   5. `values` / `valueOf` inside an `enum` — implicitly declared by the compiler, present in no
//!      source and in most indexes.
//!
//! ## What is deliberately NOT a resolution
//!
//! **A local variable of the same name.** [`crate::undefined_var`] skips on one because a local
//! genuinely shadows a field; here it would only hide a real error. Methods and variables live in
//! separate namespaces (JLS §6.5.7.1 resolves a method-invocation name against methods only), so a
//! `Runnable options` in scope does not make `options()` legal — javac rejects it, and so do we.

use std::collections::HashSet;

use bennu_java::prelude::{
    extract_symbols, static_import_targets, FileSymbols, MemberKind, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::nodes::has_generated_members;
use crate::resolve::type_binary;
use crate::scopes::{scope_is_directly_top, single_top_level_type};
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// `java.lang.Object`'s methods — callable bare from any class body. Hard-listed rather than trusted
/// to the hierarchy walk: `Object` is always reachable in principle, but an index that summarises it
/// (or a resolver mock) may not enumerate them, and the cost of being wrong here is a red squiggle on
/// `toString()`. Suppression-only, so over-inclusion is harmless.
const OBJECT_METHODS: &[&str] = &[
    "toString", "hashCode", "equals", "getClass", "clone", "finalize", "notify", "notifyAll", "wait",
];

/// The two methods the compiler adds to every `enum` (JLS §8.9.3). They appear in no source file, so
/// a source-derived index has nothing to report for them.
const ENUM_IMPLICIT_METHODS: &[&str] = &["values", "valueOf"];

/// Parse `source` and flag bare calls whose name resolves to nothing.
pub fn unresolved_call(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    unresolved_call_errors_in(root, &nodes, source, &symbols, resolver)
}

/// The tree-driven core: mirrors [`crate::undefined_var::undefined_var_errors_in`], iterating the
/// shared pre-collected `nodes` (one DFS) and reusing `root` + `symbols` + the `resolver`.
pub fn unresolved_call_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();

    // ── WHOLE-FILE guard: a parse error anywhere → the CST is untrustworthy. ─────────────────────
    if root.has_error() {
        return Vec::new();
    }

    // ── Locate the file's single TOP-LEVEL class or enum. ───────────────────────────────────────
    let Some(top) = single_top_level_type(root, bytes) else {
        return Vec::new();
    };

    // A generator annotation means the type's real member list is larger than anything we can read.
    if has_generated_members(top.node, bytes) {
        return Vec::new();
    }

    // The whole hierarchy must be known — otherwise an un-indexed base could declare the method and
    // every inherited call in the file would be a false positive.
    let Some(top_binary) = type_binary(&top.decl_name, symbols, resolver) else {
        return Vec::new();
    };
    if !hierarchy_fully_known(resolver, &top_binary) {
        return Vec::new();
    }

    // RESOLUTION 1: method names across the top type + every (fully-known) supertype.
    let mut known: HashSet<String> = HashSet::new();
    for_each_supertype(resolver, &top_binary, &mut |_bn, cm| {
        for m in &cm.methods {
            if m.kind == MemberKind::Method {
                known.insert(m.name.clone());
            }
        }
    });

    // RESOLUTION 2: every method declared in THIS file, read from the CST. The index is a snapshot
    // and the buffer is not — a method added in the editor is legal to call the instant it is typed,
    // and would otherwise be reported missing until the next re-index. Collected file-wide (nested
    // and anonymous types included) because over-collection can only suppress.
    for &n in nodes {
        if n.kind() != "method_declaration" {
            continue;
        }
        if let Some(name) = n.child_by_field_name("name") {
            if let Ok(t) = name.utf8_text(bytes) {
                known.insert(t.to_string());
            }
        }
    }

    // RESOLUTION 4 / 5: inherited-from-`Object`, and an enum's compiler-supplied pair.
    known.extend(OBJECT_METHODS.iter().map(|s| s.to_string()));
    if top.node.kind() == "enum_declaration" {
        known.extend(ENUM_IMPLICIT_METHODS.iter().map(|s| s.to_string()));
    }

    // RESOLUTION 3: names bound into the bare namespace by `import static …`. A SPECIFIC member is
    // modelled by name whether or not its owner resolves; a WILDCARD needs a fully-known owner, and
    // when it doesn't have one we cannot rule out ANY name → the whole file is skipped.
    for t in static_import_targets(&symbols.imports) {
        match t.member {
            Some(m) => {
                known.insert(m);
            }
            None => {
                if !hierarchy_fully_known(resolver, &t.owner_binary) {
                    return Vec::new();
                }
                for_each_supertype(resolver, &t.owner_binary, &mut |_bn, cm| {
                    for member in cm.methods.iter().chain(cm.fields.iter()) {
                        known.insert(member.name.clone());
                    }
                });
            }
        }
    }

    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "method_invocation" {
            continue;
        }
        // A receiver makes this someone else's question — `crate::members` infers the receiver's type
        // and asks it. `this(…)` / `super(…)` are `explicit_constructor_invocation`, a different node
        // kind, so they never arrive here at all.
        if n.child_by_field_name("object").is_some() {
            continue;
        }
        let Some(name_node) = n.child_by_field_name("name") else { continue };
        if name_node.has_error() {
            continue;
        }
        // The call must sit directly in the top type — no lambda, no nested / anonymous / local class
        // in between, any of which could declare the method we're about to call missing.
        if !scope_is_directly_top(n, top.node) {
            continue;
        }
        let Ok(name) = name_node.utf8_text(bytes) else { continue };
        if known.contains(name) {
            continue;
        }
        out.push(
            crate::check_id::CheckId::UnresolvedCall
                .at(name_node, format!("Cannot resolve method `{name}`")),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same `MapResolver` mock the members / undefined-var tests use: a `binary → members` map
    /// plus a `simple → binary` table.
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

    fn method(name: &str) -> Member {
        Member::method(name, TypeRef::simple("void"), Vec::new())
    }

    fn class(superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(TypeRef::simple),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: Default::default(),
        }
    }

    /// `com.acme.C extends Base`, `Base` declaring `inherited()`, plus `java.lang.Object` and a
    /// static-import owner (`WireMockConfiguration.options`) to model the real case.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), class(None, Vec::new()));
        members.insert(
            "com/acme/C".to_string(),
            class(Some("com/acme/Base"), vec![method("own")]),
        );
        members.insert(
            "com/acme/Base".to_string(),
            class(Some("java/lang/Object"), vec![method("inherited")]),
        );
        members.insert(
            "com/wm/WireMockConfiguration".to_string(),
            class(Some("java/lang/Object"), vec![method("options")]),
        );
        let simple = [
            ("C", "com/acme/C"),
            ("Base", "com/acme/Base"),
            ("Object", "java/lang/Object"),
            ("WireMockConfiguration", "com/wm/WireMockConfiguration"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags_with(src: &str, r: &MapResolver) -> Vec<String> {
        unresolved_call(src, r).into_iter().map(|d| d.message).collect()
    }

    /// Wrap `body` in `class C extends Base { void m() { … } }` under `com.acme`.
    fn in_method(body: &str) -> String {
        format!("package com.acme;\nclass C extends Base {{ void own() {{}} void m() {{ {body} }} }}")
    }

    fn diags(body: &str) -> Vec<String> {
        diags_with(&in_method(body), &resolver())
    }

    // ── POSITIVES (must flag) ───────────────────────────────────────────────────────────────────

    /// The case that started this: a static import that was never written.
    #[test]
    fn bare_call_with_no_binding_is_flagged() {
        let d = diags("options();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`options`"), "{d:?}");
    }

    #[test]
    fn bare_call_in_a_field_initialiser_is_flagged() {
        let src = "package com.acme;\nclass C extends Base { int x = nope(); void own() {} }";
        let d = diags_with(src, &resolver());
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`nope`"), "{d:?}");
    }

    /// A local of the same name does NOT bind a call: methods and variables are separate namespaces.
    #[test]
    fn a_local_of_the_same_name_does_not_resolve_a_call() {
        let d = diags("Runnable options = null; options();");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // ── NEGATIVES (must NOT flag) ───────────────────────────────────────────────────────────────

    #[test]
    fn own_method_is_resolved() {
        assert!(diags("own();").is_empty());
    }

    #[test]
    fn inherited_method_is_resolved() {
        assert!(diags("inherited();").is_empty());
    }

    /// The editor case: a method declared in the buffer but absent from the (stale) index.
    #[test]
    fn method_declared_in_this_file_but_not_indexed_is_resolved() {
        let src = "package com.acme;\nclass C extends Base { void own() {} void justTyped() {} void m() { justTyped(); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn specific_static_import_resolves_the_call() {
        let src = "package com.acme;\nimport static com.wm.WireMockConfiguration.options;\nclass C extends Base { void own() {} void m() { options(); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn wildcard_static_import_of_a_known_owner_resolves_the_call() {
        let src = "package com.acme;\nimport static com.wm.WireMockConfiguration.*;\nclass C extends Base { void own() {} void m() { options(); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    /// A wildcard whose owner is un-indexed could supply any name → the whole file is skipped.
    #[test]
    fn wildcard_static_import_of_an_unknown_owner_skips_the_file() {
        let src = "package com.acme;\nimport static com.mystery.Helper.*;\nclass C extends Base { void own() {} void m() { options(); anythingElse(); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn a_qualified_call_is_left_to_the_member_check() {
        assert!(diags("String s = \"x\"; s.noSuchMethod();").is_empty());
    }

    #[test]
    fn object_methods_are_resolved() {
        assert!(diags("toString(); hashCode(); getClass();").is_empty());
    }

    #[test]
    fn an_unknown_supertype_skips_the_file() {
        let mut r = resolver();
        r.members.remove("com/acme/Base");
        assert!(diags_with(&in_method("options();"), &r).is_empty());
    }

    #[test]
    fn a_parse_error_skips_the_file() {
        let src = "package com.acme;\nclass C extends Base { void m() { options(; } ";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    /// Lombok: `getName()` is legal and declared nowhere in the source.
    #[test]
    fn a_member_generating_annotation_skips_the_file() {
        let src = "package com.acme;\n@Data\nclass C extends Base { void own() {} void m() { getName(); } }";
        assert!(diags_with(src, &resolver()).is_empty());
        let qualified = "package com.acme;\n@lombok.Data\nclass C extends Base { void own() {} void m() { getName(); } }";
        assert!(diags_with(qualified, &resolver()).is_empty());
    }

    /// A call inside an anonymous class body resolves against THAT class and its supertype, neither
    /// of which we gathered. The name here is declared nowhere in the file, so only the scope guard
    /// can be what suppresses it.
    #[test]
    fn a_call_inside_an_anonymous_class_is_skipped() {
        let src = "package com.acme;\nclass C extends Base { void own() {} void m() { Runnable r = new Runnable() { public void run() { nothingAnywhere(); } }; } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn a_call_inside_a_lambda_is_skipped() {
        let src = "package com.acme;\nclass C extends Base { void own() {} void m() { Runnable r = () -> whatever(); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    /// `values()` / `valueOf(…)` exist on every enum without appearing in any source.
    #[test]
    fn enum_implicit_methods_are_resolved() {
        let mut r = resolver();
        r.members.insert("com/acme/E".to_string(), class(Some("java/lang/Object"), Vec::new()));
        r.simple.insert("E".to_string(), "com/acme/E".to_string());
        let src = "package com.acme;\nenum E { A, B; void m() { values(); valueOf(\"A\"); } }";
        assert!(diags_with(src, &r).is_empty());
    }

    #[test]
    fn two_top_level_types_skip_the_file() {
        let src = "package com.acme;\nclass C extends Base { void m() { options(); } }\nclass D {}";
        assert!(diags_with(src, &resolver()).is_empty());
    }
}
