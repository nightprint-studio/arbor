//! Resolver-backed **checked-exception widening on override** diagnostic (`"error"`). An overriding
//! method's `throws` clause may only declare checked exceptions that the overridden supertype method
//! already permits: every checked `X` the override declares must be `X <: Y` for some `Y` in the
//! super method's `throws` list (JLS §8.4.6 — an override can't broaden the checked-exception
//! contract). A checked `X` covered by no super-throws entry is a compile error: the overridden
//! method does not throw `X`.
//!
//! This mirrors [`crate::finals::final_override_errors_in`]: we find, for a source method, the
//! matching overridden method in a supertype by NAME + ERASED PARAM TYPES, then compare `throws`.
//!
//! Soundness (docs: NEVER a false positive; exception flow → EXTRA conservative; unknown = not an
//! error; when unsure, SKIP). Every positive rests on a chain of certainties, and ANY gap → SKIP:
//!
//!   1. The sub method must have an explicit `throws` clause; each declared type must RESOLVE to a
//!      binary. An unresolvable throws type → SKIP that exception (can't classify it).
//!   2. Only CHECKED exceptions are candidates ([`crate::checked_throw::is_checked`]), and only over a
//!      FULLY-KNOWN hierarchy ([`crate::walk::hierarchy_fully_known`]) so the checked verdict — and the
//!      later `reaches` negatives — are trustworthy. Unchecked (`RuntimeException`/`Error` subtypes) are
//!      NEVER flagged: they may widen freely.
//!   3. We must be CERTAIN the sub method IS an override of a specific super method: name + erased
//!      params match, every sub param type resolves, and the super method is found UNAMBIGUOUSLY. No
//!      overridden method / an unresolvable param / an AMBIGUOUS super method (two supertypes declare
//!      the same name+params with DIFFERENT throws) → SKIP (can't compare reliably).
//!   4. A checked `X` is PERMITTED iff some `Y` in the super method's `throws` satisfies
//!      `reaches(X, Y)` (X is-a Y). `X`'s hierarchy is fully known (step 2), so a `false` here is a
//!      real "not a subtype", not a conservative miss. Only an `X` permitted by NO `Y` is flagged.


use bennu_java::prelude::{FileSymbols, MemberKind, TypeResolver, Visibility};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::checked_throw::is_checked;
use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known, reaches};

/// The signature (mirrors [`crate::finals::final_override_errors_in`]): iterate the shared pre-collected
/// node slice, reuse the caller's `symbols`, resolver-backed. Flags each source method whose `throws`
/// clause widens the checked exceptions beyond what the overridden super method permits.
pub fn throws_widen_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        // Only class/enum bodies can hold an overriding method with a supertype to compare against.
        if matches!(n.kind(), "class_declaration" | "enum_declaration") {
            check_type_widening(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_type_widening(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(body) = n.child_by_field_name("body") else { return };

    // The type's direct supertypes to search for the overridden method. `java/lang/Object` is NOT
    // added here (unlike the final-override check): Object's overridable methods declare no checked
    // `throws`, so it can never PERMIT a checked exception — including it would only add SKIP-able
    // noise, never a positive. We compare against the explicit `extends` chain (walked transitively
    // by `for_each_supertype`). If the `extends` type doesn't resolve → no supers → nothing to flag.
    let Some(ext) = superclass_text(n, bytes) else { return }; // no `extends` → not an override → SKIP
    let Some(sup_bin) = type_binary(&ext, symbols, resolver) else { return }; // unresolvable → SKIP

    // Each method declared directly in this type: if it has an explicit `throws` clause, does it widen?
    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "method_declaration" {
            continue;
        }
        // static / private methods don't override — no super contract to widen. → SKIP.
        if has_keyword_modifier(m, bytes, "static") || has_keyword_modifier(m, bytes, "private") {
            continue;
        }
        check_method_widening(m, &sup_bin, bytes, symbols, resolver, out);
    }
}

fn check_method_widening(
    m: Node,
    sup_bin: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // The sub method's declared throws types, as (binary, node) — the node is the throw site to
    // underline. An empty/absent `throws` clause can never widen → nothing to do. → SKIP.
    let declared = declared_throws(m, bytes, symbols, resolver);
    if declared.is_empty() {
        return; // no `throws` clause (or none of its types resolved) → nothing to compare.
    }

    let Some(name_node) = m.child_by_field_name("name") else { return };
    let Some(name) = text(name_node, bytes) else { return };

    // The sub method's erased param binaries. If ANY param type doesn't resolve (or it's varargs) we
    // can't confirm this is an override of a specific super method → SKIP the whole method (can't be
    // sure it's an override / can't match the signature).
    let Some(params) = method_param_binaries(m, bytes, symbols, resolver) else { return };

    // Find the overridden super method UNAMBIGUOUSLY: same name + same erased params, walking the
    // supertype hierarchy. `for_each_supertype` visits every KNOWN supertype without short-circuit.
    // We collect the throws lists of ALL matches so we can detect ambiguity.
    let mut matches: Vec<Vec<String>> = Vec::new();
    for_each_supertype(resolver, sup_bin, &mut |_bn, cm| {
        for sm in &cm.methods {
            // Only a real, overridable instance method with the SAME signature is "the overridden
            // method". A static / private one isn't overridden by an instance method; ctors never are.
            let overridable = sm.kind == MemberKind::Method
                && !sm.is_static
                && sm.visibility != Visibility::Private
                && sm.name != "<init>"
                && sm.name != "<clinit>";
            if !overridable || sm.name != name {
                continue;
            }
            let sm_params: Vec<String> = sm.params.iter().map(|p| p.binary_name.clone()).collect();
            if sm_params == params {
                matches.push(sm.throws.clone());
            }
        }
    });

    // No overridden method found → not (provably) an override → SKIP.
    if matches.is_empty() {
        return;
    }
    // AMBIGUOUS super method: two supertypes declare the same name+params with DIFFERENT throws lists.
    // We can't say which contract governs → can't reliably compute "permitted" → SKIP. (Identical
    // lists across supertypes are fine — they agree on the permitted set.)
    let first = &matches[0];
    if matches.iter().any(|t| t != first) {
        return;
    }
    let super_throws = first; // the single, agreed-upon permitted set.

    // SAFETY NET (docs: NEVER a false positive). Every `y` in the permitted set must RESOLVE to a
    // known type — otherwise `reaches(x, y)` can only ever be `false` for it (an unresolvable `y` has
    // no hierarchy to match), so a `y` we can't resolve would silently drop out of the permitted set
    // and could turn a legal override into a phantom "does not permit". This bites when a PROJECT
    // super method's `throws` didn't fully resolve (e.g. an implicit-java.lang or oddly-imported type
    // the index couldn't bind): the sub's declared throw resolves fully, the super's doesn't, and they
    // never match. If ANY permitted entry is unresolvable we can't compute the permitted set reliably
    // → SKIP the whole method rather than risk a false positive.
    if super_throws.iter().any(|y| !is_binary_resolvable(y, resolver)) {
        return;
    }

    // For each CHECKED exception the sub declares that is permitted by NO super-throws entry → flag it.
    for (x_bin, x_node) in &declared {
        // Only checked exceptions can widen the contract; unchecked (RuntimeException/Error subtypes)
        // may be declared freely. Gate `is_checked` on a FULLY-KNOWN hierarchy so its verdict — and the
        // `reaches` permitted-check below — are trustworthy (no unknown link hid an unchecked ancestor,
        // no unknown link short-circuited a `reaches` to `true`). Not fully known / not checked → SKIP.
        if !hierarchy_fully_known(resolver, x_bin) {
            continue; // hierarchy gap → can't trust the checked classification → SKIP.
        }
        if !is_checked(resolver, x_bin) {
            continue; // unchecked (or not a Throwable we understand) → allowed to widen → SKIP.
        }
        // Permitted iff `x` is-a some declared super-throws `y` (`reaches(x, y)`): `throws Y` on the
        // super covers throwing any subtype of `Y`. `x`'s hierarchy is fully known, so a `false` from
        // every `y` is a REAL "not covered", not a conservative miss.
        let permitted = super_throws.iter().any(|y| reaches(resolver, x_bin, y));
        if !permitted {
            out.push(err(
                format!(
                    "`{name}` in the subclass throws `{}`, which the overridden method does not permit",
                    simple_name(x_bin)
                ),
                *x_node,
            ));
        }
    }
}

/// The sub method's `throws` clause resolved to (binary, type-node) pairs. Unresolvable throws types
/// are dropped (can't classify them → we simply don't consider them; missing one can only DROP a
/// diagnostic, never add a wrong one). `None`-of-clause / empty clause → empty vec.
fn declared_throws<'t>(
    m: Node<'t>,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<(String, Node<'t>)> {
    let mut out = Vec::new();
    let Some(throws_node) = child_of_kind(m, "throws") else { return out };
    let mut c = throws_node.walk();
    for ty in throws_node.named_children(&mut c) {
        if !is_type_node(ty.kind()) {
            continue;
        }
        let Ok(text) = ty.utf8_text(bytes) else { continue };
        // Unresolvable declared throw → drop it (can't classify → not flagged). Conservative.
        if let Some(binary) = type_binary(text, symbols, resolver) {
            out.push((binary, ty));
        }
    }
    out
}

/// The erased binary names of a method's parameter types. `None` (skip the whole method) if any
/// parameter type can't be resolved, or the method is varargs — mirrors `finals::method_param_binaries`
/// so the override-matching is byte-for-byte the same: a signature we can't fully resolve can't be
/// confirmed an override, so we SKIP rather than risk matching the wrong super method.
fn method_param_binaries(
    md: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Vec<String>> {
    let params_node = md.child_by_field_name("parameters")?;
    let mut out = Vec::new();
    let mut c = params_node.walk();
    for p in params_node.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                let ty = p.child_by_field_name("type")?;
                let text = ty.utf8_text(bytes).ok()?;
                out.push(type_binary(text, symbols, resolver)?); // unresolvable param → SKIP method.
            }
            "spread_parameter" => return None, // varargs — skip (erased-array matching is finicky).
            _ => {}
        }
    }
    Some(out)
}

/// The `extends` type text of a class (`superclass` wrapper), if any. Mirrors `finals::superclass_text`.
fn superclass_text(n: Node, bytes: &[u8]) -> Option<String> {
    let sc = n.child_by_field_name("superclass")?;
    let mut c = sc.walk();
    for ch in sc.named_children(&mut c) {
        if matches!(ch.kind(), "type_identifier" | "scoped_type_identifier" | "generic_type") {
            return text(ch, bytes);
        }
    }
    None
}

// ── CST helpers ──────────────────────────────────────────────────────────────

fn has_keyword_modifier(node: Node, bytes: &[u8], keyword: &str) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            if let Ok(t) = ch.utf8_text(bytes) {
                return t.split_whitespace().any(|w| w == keyword);
            }
        }
    }
    false
}

/// The first direct named child of `n` with the given kind.
fn child_of_kind<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == kind {
            return Some(ch);
        }
    }
    None
}

/// A type node that names a class/interface (a throws alternative).
fn is_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

fn text(node: Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(str::to_string)
}

/// The simple (last) segment of a binary name (`java/sql/SQLException` → `SQLException`).
fn simple_name(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

/// Whether `binary` names a type the resolver can actually resolve (its members are known). A raw,
/// unresolved throws word (e.g. a project super's `"Exception"` that the index left unbound) has no
/// members → `false` → the permitted-set guard SKIPs rather than mis-flag.
fn is_binary_resolvable(binary: &str, resolver: &dyn TypeResolver) -> bool {
    resolver.members_of(binary).is_some()
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic {
        message,
        severity: "error".to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap as Map;
    use std::sync::Arc;
    use tree_sitter::Parser;

    // Same mock-resolver shape as `finals.rs` / `members.rs`.
    struct MapResolver {
        members: Map<String, ClassMembers>,
        simple: Map<String, String>,
    }
    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _i: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    /// A no-superclass, no-interface, no-method exception-type node (for building the hierarchy).
    fn exc(superclass: Option<&str>) -> ClassMembers {
        ClassMembers {
            superclass: superclass.map(str::to_string),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// A `run()` method declaring the given binary `throws`.
    fn run_throwing(throws: &[&str]) -> Member {
        Member::method("run", TypeRef::simple("void"), Vec::new())
            .throws(throws.iter().map(|s| s.to_string()).collect())
    }

    /// `Base` with `void run() throws IOException`, plus the exception hierarchy
    /// `Object ← Throwable ← Exception ← { IOException ← FileNotFoundException, SQLException }` and the
    /// unchecked branch `Exception ← RuntimeException ← IllegalStateException`.
    fn resolver_with(base: ClassMembers) -> MapResolver {
        let mut members = Map::new();
        members.insert("com/acme/Base".to_string(), base);
        members.insert("java/lang/Object".into(), exc(None));
        members.insert("java/lang/Throwable".into(), exc(Some("java/lang/Object")));
        members.insert("java/lang/Exception".into(), exc(Some("java/lang/Throwable")));
        members.insert("java/io/IOException".into(), exc(Some("java/lang/Exception")));
        members.insert("java/io/FileNotFoundException".into(), exc(Some("java/io/IOException")));
        members.insert("java/sql/SQLException".into(), exc(Some("java/lang/Exception")));
        members.insert("java/lang/RuntimeException".into(), exc(Some("java/lang/Exception")));
        members.insert(
            "java/lang/IllegalStateException".into(),
            exc(Some("java/lang/RuntimeException")),
        );

        let simple = [
            ("Base", "com/acme/Base"),
            ("Object", "java/lang/Object"),
            ("Throwable", "java/lang/Throwable"),
            ("Exception", "java/lang/Exception"),
            ("IOException", "java/io/IOException"),
            ("FileNotFoundException", "java/io/FileNotFoundException"),
            ("SQLException", "java/sql/SQLException"),
            ("RuntimeException", "java/lang/RuntimeException"),
            ("IllegalStateException", "java/lang/IllegalStateException"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// The default resolver: `Base.run() throws IOException`.
    fn resolver() -> MapResolver {
        let base = ClassMembers {
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![run_throwing(&["java/io/IOException"])],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        resolver_with(base)
    }

    fn widens_with(src: &str, r: &MapResolver) -> Vec<String> {
        let symbols = bennu_java::prelude::extract_symbols(src);
        let tree = parse(src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        throws_widen_errors_in(&nodes, src, &symbols, r).into_iter().map(|d| d.message).collect()
    }

    fn widens(src: &str) -> Vec<String> {
        widens_with(src, &resolver())
    }

    // ── positives ────────────────────────────────────────────────────────────

    #[test]
    fn widening_with_unrelated_checked_is_flagged() {
        // SQLException is not a subtype of IOException (the only permitted throw) → widening.
        let d = widens("class Sub extends Base { @Override void run() throws java.sql.SQLException {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(
            d[0].contains("SQLException") && d[0].contains("does not permit"),
            "{d:?}"
        );
    }

    // ── negatives ────────────────────────────────────────────────────────────

    #[test]
    fn same_checked_exception_is_ok() {
        // `throws IOException` == the super's permitted throw → not widening.
        assert!(widens("class Sub extends Base { void run() throws IOException {} }").is_empty());
    }

    #[test]
    fn subtype_of_permitted_is_ok() {
        // FileNotFoundException <: IOException → covered by the super's `throws IOException`.
        assert!(
            widens("class Sub extends Base { void run() throws java.io.FileNotFoundException {} }")
                .is_empty()
        );
    }

    #[test]
    fn unchecked_exception_is_never_flagged() {
        // IllegalStateException is a RuntimeException subtype (unchecked) → may widen freely.
        assert!(
            widens("class Sub extends Base { void run() throws IllegalStateException {} }").is_empty()
        );
    }

    #[test]
    fn super_throws_supertype_covers_subtype() {
        // Super declares `throws Exception`; SQLException <: Exception → permitted, not flagged.
        let base = ClassMembers {
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![run_throwing(&["java/lang/Exception"])],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let r = resolver_with(base);
        assert!(
            widens_with("class Sub extends Base { void run() throws SQLException {} }", &r).is_empty()
        );
    }

    #[test]
    fn no_overridden_method_is_not_flagged() {
        // `other(...)` isn't declared by Base → no override found → SKIP (can't compare).
        assert!(
            widens("class Sub extends Base { void other() throws SQLException {} }").is_empty()
        );
    }

    #[test]
    fn different_signature_is_not_an_override() {
        // `run(int)` ≠ Base.run() (erased params differ) → not an override → SKIP.
        assert!(
            widens("class Sub extends Base { void run(int x) throws SQLException {} }").is_empty()
        );
    }

    #[test]
    fn hierarchy_not_fully_known_is_not_flagged() {
        // The declared throw resolves but extends an UNKNOWN base → checked classification untrustworthy
        // → SKIP.
        let base = ClassMembers {
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![run_throwing(&["java/io/IOException"])],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let mut r = resolver_with(base);
        r.members
            .insert("com/acme/Weird".into(), exc(Some("com/acme/UnknownBase")));
        r.simple.insert("Weird".into(), "com/acme/Weird".into());
        assert!(
            widens_with("class Sub extends Base { void run() throws Weird {} }", &r).is_empty()
        );
    }

    #[test]
    fn no_throws_clause_is_not_flagged() {
        // Overriding with a NARROWER (empty) throws is always legal.
        assert!(widens("class Sub extends Base { void run() {} }").is_empty());
    }

    #[test]
    fn no_extends_is_not_flagged() {
        // No superclass → nothing to override → SKIP.
        assert!(widens("class Sub { void run() throws SQLException {} }").is_empty());
    }

    #[test]
    fn unresolvable_supertype_is_not_flagged() {
        // `extends Mystery` doesn't resolve → no super method to compare → SKIP.
        assert!(
            widens("class Sub extends Mystery { void run() throws SQLException {} }").is_empty()
        );
    }

    #[test]
    fn static_method_is_not_an_override() {
        // A static method of the same name doesn't override → SKIP.
        assert!(
            widens("class Sub extends Base { static void run() throws SQLException {} }").is_empty()
        );
    }

    #[test]
    fn super_throws_with_an_unresolvable_entry_is_not_flagged() {
        // THE regression: a PROJECT super method whose `throws` didn't fully resolve — its permitted
        // set is the raw word `"Exception"` (no such binary in the index), while the sub's declared
        // `throws Exception` resolves fully to `java/lang/Exception`. They never match, so a naive
        // check would flag "does not permit Exception" on perfectly legal code. The permitted-set
        // resolvability guard must SKIP instead (never a false positive).
        let base = ClassMembers {
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            // Unresolved throws entry: `"Exception"` is NOT a key in the mock resolver's members.
            methods: vec![run_throwing(&["Exception"])],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let r = resolver_with(base);
        assert!(
            widens_with("class Sub extends Base { void run() throws Exception {} }", &r).is_empty(),
            "an unresolvable permitted entry ⇒ SKIP, never flag",
        );
    }

    #[test]
    fn ambiguous_super_method_is_not_flagged() {
        // Base and an interface Mixin both declare `run()` with DIFFERENT throws (IOException vs
        // SQLException) → ambiguous permitted set → SKIP (can't say which governs).
        let base = ClassMembers {
            superclass: Some("com/acme/Mixin".to_string()),
            interfaces: Vec::new(),
            methods: vec![run_throwing(&["java/io/IOException"])],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let mut r = resolver_with(base);
        r.members.insert(
            "com/acme/Mixin".into(),
            ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![run_throwing(&["java/sql/SQLException"])],
                fields: Vec::new(),
                flags: ClassFlags { is_interface: true, ..ClassFlags::default() },
            },
        );
        // `throws SQLException` would widen w.r.t. Base's IOException, but Mixin permits SQLException →
        // ambiguous → SKIP.
        assert!(
            widens_with("class Sub extends Base { void run() throws SQLException {} }", &r).is_empty()
        );
    }
}
