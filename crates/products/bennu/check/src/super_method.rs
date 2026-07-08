//! Unknown-super-method diagnostics — a `super.foo(...)` call whose `foo` exists **nowhere** in the
//! enclosing class's superclass/interface hierarchy. Resolver-backed sibling of [`crate::members`]
//! (unknown method on an inferred receiver): where `members` resolves the *receiver's* type, here the
//! "receiver" is the enclosing class's super-hierarchy, resolved from its `extends`/`implements`.
//!
//! Conservative — never a false positive (see the module docs of [`crate::walk`]):
//!   * only a literal `super.NAME(...)` is checked; `this.`, a qualified `Outer.super.x`, or a bare
//!     `foo()` are skipped (a qualified super needs the enclosing *outer* type, harder to pin down);
//!   * we resolve the enclosing class's declared superclass + interfaces; if the superclass text is
//!     unresolvable, OR any part of the reachable super-hierarchy is unknown
//!     (`hierarchy_fully_known` is `false`), we stay silent — an un-indexed base might declare it;
//!   * an implicit-`Object` superclass (no `extends`) is only checked when `java/lang/Object` itself
//!     resolves; otherwise silent;
//!   * matching is by method NAME only — an overload / generic signature never causes a wrong
//!     "cannot resolve", a name match anywhere in the hierarchy clears the call.

use std::collections::HashSet;

use bennu_java::prelude::{FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// Parse `source` and flag `super.foo(...)` calls whose `foo` is absent from the super-hierarchy.
pub fn super_method_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
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
    super_method_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core — mirrors [`crate::members::unknown_members_in`]'s arg list so it wires the same
/// way into `check_file_resolved`. `root` and `cache` are accepted for signature parity with the
/// other resolver-backed checks; this check needs neither (it walks parents from each node and does
/// no type inference), so both are unused — wire it with `_root` / a throwaway cache if preferred.
pub fn super_method_errors_in(
    _root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    _cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "method_invocation" {
            check_call(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_call(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // Only `super.NAME(...)` — the `object` field must be a bare `super` node. A missing object (bare
    // call), a `this` object, an expression object, or a qualified `Outer.super` (whose object is a
    // `super` node WRAPPED under a scoped expression, not the direct `super` kind) are all skipped.
    let Some(obj) = n.child_by_field_name("object") else { return };
    if obj.kind() != "super" {
        return; // not a plain `super.` receiver (covers `this.`, `x.`, `Outer.super.` — SKIP)
    }
    let Some(name) = n.child_by_field_name("name") else { return };
    if name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };

    // Locate the enclosing class: the nearest ancestor `class_declaration`. For a `super.` inside an
    // anonymous class body the nearest enclosing type is that anonymous class (its supertype is the
    // `object_creation_expression`'s type) — we can't cheaply resolve that written supertype here, so
    // an anonymous-class `super.` is skipped by `enclosing_class` returning the wrong (outer) node
    // guarded below: we only proceed for a real `class_declaration` whose supertypes we can read.
    let Some(class) = enclosing_class(n) else { return };
    // An anonymous class body has no `class_declaration` ancestor between it and the outer class; if
    // the `super.` sits inside an `object_creation_expression`'s body, the enclosing *type* is that
    // anonymous class, not `class`. Detect that interposed anon body and SKIP (can't resolve its super).
    if inside_anonymous_class_below(n, class) {
        return;
    }

    let cls_name = class_name(class, bytes).unwrap_or("this class");

    // Direct super-hierarchy roots: the declared superclass (or implicit Object) + every interface.
    let Some(super_roots) = super_roots(class, bytes, symbols, resolver) else {
        return; // superclass text unresolvable → SKIP (can't rule the method out)
    };

    // Every reachable supertype must be known — else the collected name set is incomplete and a
    // "nowhere" conclusion could be wrong. Bail on any gap (mirrors `missing_abstract_impls`).
    if !super_roots.iter().all(|r| hierarchy_fully_known(resolver, r)) {
        return;
    }

    // Collect every method name across the whole (fully-known) super-hierarchy. A NAME match — not
    // arity/type — clears the call, so overloads/generics never produce a false positive.
    let mut names: HashSet<String> = HashSet::new();
    for root in &super_roots {
        for_each_supertype(resolver, root, &mut |_bn, cm| {
            for m in &cm.methods {
                names.insert(m.name.clone());
            }
        });
    }

    if !names.contains(method) {
        out.push(crate::check_id::CheckId::UnresolvedSuperMethod.at(
            name,
            format!(
                "Cannot resolve method `{method}` in the superclass of `{}`",
                simple_name(cls_name)
            ),
        ));
    }
}

/// Binary names of the enclosing class's direct super-hierarchy roots: its declared superclass (or
/// `java/lang/Object` when there's no explicit `extends`, but only if Object resolves) + every
/// declared interface. `None` when the superclass text is present but unresolvable, OR when an
/// implicit-Object base can't be resolved — both mean "can't assert absence", so the caller SKIPs.
fn super_roots(
    class: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Vec<String>> {
    let mut roots: Vec<String> = Vec::new();

    match superclass_text(class, bytes) {
        Some(text) => {
            // An explicit `extends S`: S must resolve to a binary name, else we can't walk it. SKIP.
            let bin = type_binary(&text, symbols, resolver)?;
            roots.push(bin);
        }
        None => {
            // No `extends` → the superclass is implicitly `java/lang/Object`. Only usable when Object
            // itself resolves (its method set is what a `super.foo()` here would resolve against);
            // otherwise we know nothing about the super and must SKIP.
            if resolver.members_of("java/lang/Object").is_none() {
                return None;
            }
            roots.push("java/lang/Object".to_string());
        }
    }

    // Interfaces contribute their default (and, harmlessly for name-matching, abstract) methods. An
    // unresolvable interface text is left out — it's caught by the `hierarchy_fully_known` gate the
    // caller runs on the collected roots (a resolvable-but-incomplete interface hierarchy bails there).
    for text in interface_texts(class, bytes) {
        if let Some(bin) = type_binary(&text, symbols, resolver) {
            roots.push(bin);
        } else {
            return None; // a written interface we can't resolve → incomplete picture → SKIP
        }
    }

    Some(roots)
}

// ── CST helpers ──────────────────────────────────────────────────────────────

/// The nearest ancestor `class_declaration` of `n` (its enclosing class). `None` for a `super.` that
/// isn't inside any class declaration (e.g. a malformed buffer).
fn enclosing_class(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if p.kind() == "class_declaration" {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// Whether an anonymous class body sits between `n` and its enclosing `class_declaration`. When true,
/// the *type* that owns the `super.` is the anonymous class (whose supertype we don't resolve here),
/// so the caller SKIPs rather than resolving against the wrong (outer) class.
fn inside_anonymous_class_below(n: Node, class: Node) -> bool {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if p.id() == class.id() {
            return false; // reached the enclosing class with no anon body in between
        }
        if p.kind() == "object_creation_expression" {
            // A `new T(){ ... }` with a body means everything below it is in an anonymous class.
            let mut c = p.walk();
            if p.children(&mut c).any(|ch| ch.kind() == "class_body") {
                return true;
            }
        }
        cur = p.parent();
    }
    false
}

/// The written superclass type of a `class_declaration` (`extends S`), if any. tree-sitter-java wraps
/// it in a `superclass` field whose first type node is `S`.
fn superclass_text(class: Node, bytes: &[u8]) -> Option<String> {
    let w = class.child_by_field_name("superclass")?;
    let mut c = w.walk();
    for ch in w.named_children(&mut c) {
        if is_type_node(ch.kind()) {
            return ch.utf8_text(bytes).ok().map(str::to_string);
        }
    }
    None
}

/// The written interface types of a `class_declaration` (`implements I, J`) — under the `interfaces`
/// field → a (possibly nested) `type_list`.
fn interface_texts(class: Node, bytes: &[u8]) -> Vec<String> {
    let Some(w) = class.child_by_field_name("interfaces") else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![w];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_list" || node.kind() == "interface_type_list" {
            let mut c = node.walk();
            for ch in node.named_children(&mut c) {
                if is_type_node(ch.kind()) {
                    if let Ok(t) = ch.utf8_text(bytes) {
                        out.push(t.to_string());
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

fn class_name<'a>(class: Node, bytes: &'a [u8]) -> Option<&'a str> {
    class.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// A fixed resolver: a `binary → members` map + a `simple → binary` table (same shape as the
    /// unknown-member / inheritance tests).
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

    /// Object (with `toString`) and a `Base extends Object` (with `greet`). `Base` is the seed
    /// superclass every "super.X" test resolves against.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method("toString", "java/lang/String")],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Base".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![method("greet", "void")],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        let simple = [
            ("Object", "java/lang/Object"),
            ("Base", "com/acme/Base"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags_with(src: &str, r: &MapResolver) -> Vec<String> {
        super_method_errors(src, r).into_iter().map(|d| d.message).collect()
    }
    fn diags(src: &str) -> Vec<String> {
        diags_with(src, &resolver())
    }

    #[test]
    fn existing_super_method_is_ok() {
        // `greet` is declared on Base (the superclass) → resolvable, no diagnostic.
        assert!(diags("class C extends Base { void m() { super.greet(); } }").is_empty());
    }

    #[test]
    fn missing_super_method_is_flagged() {
        // `nope` exists nowhere in Base → Object → flag.
        let d = diags("class C extends Base { void m() { super.nope(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("nope") && d[0].contains('C'), "{d:?}");
    }

    #[test]
    fn unresolvable_superclass_is_not_flagged() {
        // `Mystery` doesn't resolve → we can't walk the super-hierarchy → SKIP.
        assert!(diags("class C extends Mystery { void m() { super.nope(); } }").is_empty());
    }

    #[test]
    fn unknown_link_in_hierarchy_is_not_flagged() {
        // Base resolves, but its superclass `Gap` is unknown → hierarchy not fully known → SKIP.
        let mut r = resolver();
        r.members.insert(
            "com/acme/Leaky".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("com/acme/Gap".to_string()), // Gap is never seeded → unknown link
                interfaces: Vec::new(),
                methods: vec![method("here", "void")],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        r.simple.insert("Leaky".to_string(), "com/acme/Leaky".to_string());
        assert!(diags_with("class C extends Leaky { void m() { super.nope(); } }", &r).is_empty());
    }

    #[test]
    fn object_method_via_implicit_super_is_ok() {
        // No `extends` → implicit Object; `toString` is an Object method → resolvable, no diagnostic.
        assert!(diags("class C { void m() { super.toString(); } }").is_empty());
    }

    #[test]
    fn missing_method_via_implicit_object_is_flagged() {
        // No `extends`, Object resolves, `nope` isn't an Object method → flag.
        let d = diags("class C { void m() { super.nope(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("nope"), "{d:?}");
    }

    #[test]
    fn implicit_object_unresolvable_is_not_flagged() {
        // No `extends` AND Object doesn't resolve → we know nothing about the super → SKIP.
        let r = MapResolver { members: HashMap::new(), simple: HashMap::new() };
        assert!(diags_with("class C { void m() { super.nope(); } }", &r).is_empty());
    }

    #[test]
    fn non_super_receiver_is_ignored() {
        // `this.nope()` and a bare `nope()` are not `super.` calls → never checked here.
        assert!(diags("class C extends Base { void m() { this.nope(); nope(); } }").is_empty());
    }
}
