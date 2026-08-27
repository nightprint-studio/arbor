//! Two resolver-backed inheritance diagnostics, both **errors**:
//!
//!   * **`cyclic_inheritance`** — a type that transitively extends/implements ITSELF
//!     (`class A extends B`, `class B extends A`; or `class A extends A`). Detected by resolving the
//!     declared type to a binary name, then walking its supertypes via the resolver: if the walk
//!     returns to the starting binary name, the loop is closed → cycle.
//!   * **`override_overrides_nothing`** — a method annotated `@Override` (or `@java.lang.Override`)
//!     that overrides/implements NOTHING in any supertype (the clear typo case).
//!
//! PARAMOUNT RULE — never a false positive. These are the *extra*-conservative guards:
//!
//!   * **cycle**: every link in the cycle must be a resolvable type AND the walk must actually return
//!     to the starting binary name. An unresolvable type in the walk (`members_of` → `None`) is a
//!     dead end that CANNOT close a loop — we never infer a cycle through an unknown type. A depth cap
//!     + visited-set guard the walk (mirrors the existing hierarchy walkers). We flag only the
//!     declared type whose own name closes the loop, and only once per type.
//!   * **override**: we flag ONLY when (a) the enclosing type's ENTIRE supertype hierarchy is fully
//!     resolvable ([`hierarchy_fully_known`] over every direct supertype), AND (b) NO supertype
//!     declares ANY method of that name. Matching is by name only for the "flag" decision — if a
//!     supertype has a method of the same name with ANY arity, we treat it as "might override" and
//!     SKIP (name+arity/erased-signature differences are legal covariant/generic overrides we must
//!     not mis-report). If any supertype is unresolvable, or the enclosing type itself is
//!     unresolvable, we SKIP. This yields the single safe case: an `@Override` on a name that exists
//!     NOWHERE in a fully-known hierarchy — an unambiguous typo.

use std::collections::HashSet;

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::nodes::simple_name;
use crate::resolve::type_binary;
use crate::walk::hierarchy_fully_known;

/// Depth guard against a pathological hierarchy (cycles are also caught by the visited-set). Mirrors
/// `walk::MAX_DEPTH`.
const MAX_DEPTH: usize = 40;

/// Parse `source` and flag cyclic inheritance + `@Override`-overrides-nothing.
pub fn inherit_cycle_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let symbols = bennu_java::prelude::extract_symbols(source);
    with_parse(source, |root| {
        inherit_cycle_errors_in(&crate::check::collect_nodes(root), source, &symbols, resolver)
    })
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`. Mirrors
/// `inheritance_errors_in`'s signature so the `check_file_resolved` aggregator can call it the same way.
pub fn inherit_cycle_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => {
                check_cycle(n, bytes, symbols, resolver, &mut out);
                check_overrides(n, bytes, symbols, resolver, &mut out);
            }
            _ => {}
        }
    }
    out
}

// ── check 1: cyclic inheritance ──────────────────────────────────────────────

/// Flag `n` when its declared type transitively extends/implements ITSELF, and every link in the
/// loop is resolvable. The declared type's own binary name is the target: we walk its resolved
/// supertypes and only flag if we return to that exact name.
fn check_cycle(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // The declared type's own binary name — the loop target. `symbols.types` is authoritative for a
    // same-file declaration; fall back to the resolver via `type_binary`.
    let Some(name_node) = n.child_by_field_name("name") else { return };
    let Some(simple) = name_node.utf8_text(bytes).ok() else { return };
    let Some(self_bin) = type_binary(simple, symbols, resolver) else { return };

    // Resolve the declared type's members so we start the walk from its ACTUAL supertypes. If the
    // type isn't resolvable, we can't know its supertypes → skip (never guess a cycle).
    let Some(cm) = resolver.members_of(&self_bin) else { return };

    // Direct supertypes (superclass + interfaces) as the walk's seeds.
    let mut seeds: Vec<String> = Vec::new();
    if let Some(sc) = &cm.superclass {
        seeds.push(sc.clone());
    }
    seeds.extend(cm.interfaces.iter().cloned());

    // From any seed, can we reach `self_bin` again (through only-resolvable links)? If so, the loop
    // closes → cycle. `reaches_self` returns `false` on any unknown link, so a cycle can never be
    // inferred through an unresolvable type.
    let mut visited: HashSet<String> = HashSet::new();
    let closes = seeds.iter().any(|s| reaches_self(resolver, s, &self_bin, &mut visited, 1));
    if closes {
        out.push(CheckId::CyclicInheritance.at(
            name_node,
            format!("Cyclic inheritance involving `{}`", simple_name(&self_bin)),
        ));
    }
}

/// Whether the supertype walk starting at `from` returns to `target` (the type whose cycle we're
/// testing), traversing ONLY resolvable links. Conservative inversion of `walk::reaches`: an unknown
/// class is a DEAD END (`false`), never a match — a cycle must be proven through fully-known links.
/// `visited` guards against non-target cycles among the seeds; the depth cap bounds pathological input.
fn reaches_self(
    resolver: &dyn TypeResolver,
    from: &str,
    target: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> bool {
    if from == target {
        return true; // closed the loop back to the starting type
    }
    if depth > MAX_DEPTH || !visited.insert(from.to_string()) {
        return false;
    }
    let Some(cm) = resolver.members_of(from) else {
        return false; // unknown link → cannot close a cycle through it
    };
    if let Some(sc) = &cm.superclass {
        if reaches_self(resolver, sc, target, visited, depth + 1) {
            return true;
        }
    }
    cm.interfaces.iter().any(|i| reaches_self(resolver, i, target, visited, depth + 1))
}

// ── check 2: @Override overrides nothing ─────────────────────────────────────

/// Flag each `@Override` method in `n`'s body that overrides/implements nothing — but ONLY when `n`'s
/// entire supertype hierarchy is fully known, and only for a method whose name exists nowhere in that
/// hierarchy (the unambiguous typo case).
fn check_overrides(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(body) = n.child_by_field_name("body") else { return };

    // Direct supertypes (extends + implements) as binary names. An unresolvable supertype means the
    // hierarchy is incomplete → we can't assert "overrides nothing" → skip the WHOLE type.
    let mut supers: Vec<String> = Vec::new();
    for text in direct_supertype_texts(n, bytes) {
        match type_binary(&text, symbols, resolver) {
            Some(bin) => supers.push(bin),
            None => return, // unresolvable supertype → bail (conservative)
        }
    }
    if supers.is_empty() {
        return; // no explicit supertype (only Object) → an @Override can only mean an Object method;
                // we don't have Object's method table guaranteed, so nothing to assert here safely.
    }
    // Every reachable supertype must be fully resolvable, else the method set is incomplete and a
    // real override could be hiding in an un-indexed base.
    if !supers.iter().all(|s| hierarchy_fully_known(resolver, s)) {
        return;
    }

    // The set of ALL method names declared anywhere in the (fully-known) supertype hierarchy. Because
    // the hierarchy is fully known, a name absent here is DEFINITELY not overridable → a real typo.
    let mut super_method_names: HashSet<String> = HashSet::new();
    for s in &supers {
        crate::walk::for_each_supertype(resolver, s, &mut |_bn, cm| {
            for m in &cm.methods {
                super_method_names.insert(m.name.clone());
            }
        });
    }

    // Each method declared directly in this type's body: if it's `@Override` and its name appears in
    // NO supertype, flag it.
    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "method_declaration" {
            continue;
        }
        if !has_override_annotation(m, bytes) {
            continue;
        }
        let Some(name_node) = m.child_by_field_name("name") else { continue };
        let Some(name) = name_node.utf8_text(bytes).ok() else { continue };
        // Name matches SOME supertype method (any arity) → might override → SKIP (conservative:
        // covariant returns / generic-erasure signature differences are legal overrides).
        if super_method_names.contains(name) {
            continue;
        }
        out.push(CheckId::OverrideOverridesNothing.at(
            name_node,
            "Method does not override or implement a method from a supertype",
        ));
    }
}

/// Whether a method declaration carries `@Override` / `@java.lang.Override` in its `modifiers`.
fn has_override_annotation(md: Node, bytes: &[u8]) -> bool {
    let mut c = md.walk();
    for ch in md.children(&mut c) {
        if ch.kind() != "modifiers" {
            continue;
        }
        let mut mc = ch.walk();
        for a in ch.children(&mut mc) {
            if matches!(a.kind(), "marker_annotation" | "annotation") {
                if let Some(name) = a.child_by_field_name("name") {
                    if let Ok(t) = name.utf8_text(bytes) {
                        // Simple name (last segment of a possibly-qualified annotation).
                        if t.rsplit('.').next().unwrap_or(t) == "Override" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

// ── CST helpers ──────────────────────────────────────────────────────────────

/// The `(text)` of every direct supertype of a class/enum/record/interface: the `extends` type(s)
/// plus the `implements` types. For an interface the `extends_interfaces` list folds in here too.
fn direct_supertype_texts(n: Node, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    // `extends S` on a class: `superclass` wrapper → the type node under it.
    if let Some(w) = n.child_by_field_name("superclass") {
        collect_type_texts(w, bytes, &mut out);
    }
    // `implements I, J` on a class/enum/record: `interfaces` wrapper → `type_list`.
    if let Some(w) = n.child_by_field_name("interfaces") {
        collect_type_texts(w, bytes, &mut out);
    }
    // `extends I, J` on an interface: an `extends_interfaces` child (no field name).
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == "extends_interfaces" {
            collect_type_texts(ch, bytes, &mut out);
        }
    }
    out
}

/// Collect the text of every type node under `wrapper` (recurses through `type_list` / wrappers).
fn collect_type_texts(wrapper: Node, bytes: &[u8], out: &mut Vec<String>) {
    let mut stack = vec![wrapper];
    while let Some(node) = stack.pop() {
        if is_class_type_node(node.kind()) {
            if let Ok(t) = node.utf8_text(bytes) {
                out.push(t.to_string());
            }
            continue; // a generic type's args are children — don't descend into them as supertypes
        }
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            stack.push(ch);
        }
    }
}

/// Whether a kind is a REFERENCE type as written — the only thing an `extends`, `implements` or
/// `throws` list can hold. Primitives and arrays are excluded on purpose; see
/// `erasure_clash::is_written_type_node` for the predicate that includes them.
fn is_class_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

fn with_parse(source: &str, f: impl FnOnce(Node) -> Vec<Diagnostic>) -> Vec<Diagnostic> {
    match bennu_java::prelude::parse_java(source) {
        Some(tree) => f(tree.root_node()),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    // Same mock shape as inheritance.rs / finals.rs tests: a `binary → members` map + a
    // `simple → binary` table.
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

    fn cm(
        superclass: Option<&str>,
        ifaces: &[&str],
        methods: Vec<Member>,
        flags: ClassFlags,
    ) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: ifaces.iter().map(|s| s.to_string()).collect(),
            methods,
            fields: Vec::new(),
            flags,
        }
    }

    fn method(name: &str, params: &[&str]) -> Member {
        let params = params.iter().map(|p| TypeRef::simple(p.to_string())).collect();
        Member::method(name, TypeRef::simple("void"), params)
    }

    fn iface_flags() -> ClassFlags {
        let mut f = ClassFlags::default();
        f.is_interface = true;
        f
    }

    fn diags(src: &str, r: &MapResolver) -> Vec<String> {
        inherit_cycle_errors(src, r).into_iter().map(|d| d.message).collect()
    }

    // ── cyclic inheritance ─────────────────────────────────────────────────────

    /// A genuine 2-type cycle: `A extends B`, `B extends A` (both project types, both resolvable).
    #[test]
    fn genuine_two_type_cycle_is_flagged() {
        let mut members = HashMap::new();
        members.insert("com/acme/A".to_string(), cm(Some("com/acme/B"), &[], vec![], ClassFlags::default()));
        members.insert("com/acme/B".to_string(), cm(Some("com/acme/A"), &[], vec![], ClassFlags::default()));
        let simple = [("A", "com/acme/A"), ("B", "com/acme/B")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        // The declared type in-source is `A` (its fqn resolves to com/acme/A via same-file symbols),
        // but we drive the resolver's own supertypes; give the source a matching package.
        let src = "package com.acme; class A extends B {}";
        let d = diags(src, &r);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Cyclic inheritance") && d[0].contains("`A`"), "{d:?}");
    }

    /// A normal linear hierarchy `A extends B extends Object` does NOT flag.
    #[test]
    fn linear_hierarchy_is_not_flagged() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        members.insert("com/acme/B".to_string(), cm(Some("java/lang/Object"), &[], vec![], ClassFlags::default()));
        members.insert("com/acme/A".to_string(), cm(Some("com/acme/B"), &[], vec![], ClassFlags::default()));
        let simple = [("A", "com/acme/A"), ("B", "com/acme/B"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        assert!(diags("package com.acme; class A extends B {}", &r).is_empty());
    }

    /// A chain through an UNKNOWN type does NOT flag — a cycle can't be proven through an unresolvable
    /// link, even if the source *looks* self-referential via an unindexed base.
    #[test]
    fn cycle_through_unknown_type_is_not_flagged() {
        let mut members = HashMap::new();
        // A extends Mystery (unresolved); Mystery is NOT in the map. No way to close a loop.
        members.insert("com/acme/A".to_string(), cm(Some("com/acme/Mystery"), &[], vec![], ClassFlags::default()));
        let simple = [("A", "com/acme/A")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        assert!(diags("package com.acme; class A extends Mystery {}", &r).is_empty());
    }

    /// Direct self-extension `A extends A` (resolvable) IS a cycle.
    #[test]
    fn direct_self_cycle_is_flagged() {
        let mut members = HashMap::new();
        members.insert("com/acme/A".to_string(), cm(Some("com/acme/A"), &[], vec![], ClassFlags::default()));
        let simple = [("A", "com/acme/A")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        let d = diags("package com.acme; class A extends A {}", &r);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Cyclic inheritance"), "{d:?}");
    }

    // ── @Override overrides nothing ─────────────────────────────────────────────

    /// `@Override` on a method that really overrides a known supertype method does NOT flag.
    #[test]
    fn override_of_real_supertype_method_is_ok() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        members.insert(
            "com/acme/Base".to_string(),
            cm(Some("java/lang/Object"), &[], vec![method("run", &[])], ClassFlags::default()),
        );
        let simple = [("Base", "com/acme/Base"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        let src = "package com.acme; class X extends Base { @Override public void run() {} }";
        assert!(diags(src, &r).is_empty(), "{:?}", diags(src, &r));
    }

    /// `@Override` on a method whose name exists NOWHERE in a fully-known hierarchy DOES flag.
    #[test]
    fn override_of_absent_method_is_flagged() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        members.insert(
            "com/acme/Base".to_string(),
            cm(Some("java/lang/Object"), &[], vec![method("run", &[])], ClassFlags::default()),
        );
        let simple = [("Base", "com/acme/Base"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        // `runn` is a typo: no `runn` anywhere in {X's supers} = {Base, Object}.
        let src = "package com.acme; class X extends Base { @Override public void runn() {} }";
        let d = diags(src, &r);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("does not override or implement"), "{d:?}");
    }

    /// Same name, different arity across the hierarchy → we still SKIP (name match = might override).
    #[test]
    fn same_name_different_arity_is_not_flagged() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        members.insert(
            "com/acme/Base".to_string(),
            cm(Some("java/lang/Object"), &[], vec![method("run", &[])], ClassFlags::default()),
        );
        let simple = [("Base", "com/acme/Base"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        // run(int) vs Base.run() — different arity, but name matches → conservative skip.
        let src = "package com.acme; class X extends Base { @Override public void run(int a) {} }";
        assert!(diags(src, &r).is_empty(), "{:?}", diags(src, &r));
    }

    /// `@Override` where a supertype is UNRESOLVABLE does NOT flag (conservative).
    #[test]
    fn override_with_unresolvable_supertype_is_not_flagged() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        // Base extends an unindexed `Mystery` → hierarchy not fully known.
        members.insert(
            "com/acme/Base".to_string(),
            cm(Some("com/acme/Mystery"), &[], vec![], ClassFlags::default()),
        );
        let simple = [("Base", "com/acme/Base"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        let src = "package com.acme; class X extends Base { @Override public void whatever() {} }";
        assert!(diags(src, &r).is_empty(), "{:?}", diags(src, &r));
    }

    /// Qualified `@java.lang.Override` is recognised the same as `@Override`.
    #[test]
    fn qualified_override_annotation_is_recognised() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        members.insert(
            "com/acme/Base".to_string(),
            cm(Some("java/lang/Object"), &[], vec![], ClassFlags::default()),
        );
        let simple = [("Base", "com/acme/Base"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        let src = "package com.acme; class X extends Base { @java.lang.Override public void gone() {} }";
        let d = diags(src, &r);
        assert_eq!(d.len(), 1, "{d:?}");
    }

    /// An `@Override` implementing an interface method does NOT flag.
    #[test]
    fn override_implementing_interface_method_is_ok() {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(None, &[], vec![], ClassFlags::default()));
        members.insert(
            "com/acme/Task".to_string(),
            cm(None, &[], vec![method("execute", &[])], iface_flags()),
        );
        let simple = [("Task", "com/acme/Task"), ("Object", "java/lang/Object")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        let r = MapResolver { members, simple };
        let src = "package com.acme; class X implements Task { @Override public void execute() {} }";
        assert!(diags(src, &r).is_empty(), "{:?}", diags(src, &r));
    }
}
