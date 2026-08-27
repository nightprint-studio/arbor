//! Member-visibility diagnostics — a `receiver.member` (or `Type.staticMember`) that reaches a member
//! the use site is NOT allowed to see. The visibility sibling of [`crate::members`]/[`crate::fields`]:
//! same receiver-type inference, same conservative supertype walk, but instead of "does the member
//! exist?" it asks "is the member VISIBLE here?" and flags only the two cases Java rules make sound to
//! decide from an AST + a member index.
//!
//! Two cases, both **error**:
//!   1. **private access from another top-level type** — `other.secret_value` where `secret_value` is
//!      `private` in its declaring type and the access is lexically OUTSIDE that declaring type's
//!      top-level class. (A nested class may touch its outer's privates, so the identity compared is
//!      the TOP-LEVEL type, not the immediate one.)
//!   2. **package-private access from another package** — a `Package` (default) member whose declaring
//!      type does not live in the accessing file's package. "Lives in" and not "has the same package
//!      as": a project nested type's binary joins its nesting with `/` (`com/acme/Outer/Inner`), the
//!      same separator a package uses, so there is no package to extract and compare — see the note at
//!      the `Package` arm.
//!
//! ## Never a false positive (this is accessibility — when unsure, SKIP)
//! Every gate below is a SKIP, not a flag:
//!   * no explicit receiver (bare `member`) → SKIP;
//!   * receiver type doesn't infer to a resolvable type (value receiver) / `Type` doesn't resolve
//!     (static receiver) → SKIP;
//!   * receiver type is a JDK/library type (`java/…`, `javax/…`, …) → SKIP (we don't police JDK
//!     visibility — too many nuances: module exports, `@jdk.internal`, etc.);
//!   * receiver type — or the member's declaring type — is NOT a project-source type (a dependency
//!     jar) → SKIP: a library member's true accessibility (generated accessors, split-package legacy
//!     frameworks) is as unmodellable from bytecode as the JDK's. We police only the user's own code;
//!   * the member doesn't resolve UNAMBIGUOUSLY to exactly one declaration across a FULLY-KNOWN
//!     hierarchy → SKIP (an unknown supertype might declare a public one that shadows it);
//!   * the resolved member is `Public` or `Protected` → never flagged (protected subclass rules are
//!     subtle → skipped entirely);
//!   * CASE 1: the access IS inside the declaring top-level type → legal → SKIP;
//!   * CASE 2: the declaring type lives under the accessing package, or the package is unknown → SKIP.

use bennu_java::prelude::{
    infer_node_type_cached, FileSymbols, InferCache, Member, TypeResolver, Visibility,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::nodes::simple_name;
use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// The tree-driven core (mirrors [`crate::members::unknown_members_in`]): iterate the shared `nodes`,
/// reuse `root` + `symbols` + the inference `cache`. Visits `method_invocation` (private/package
/// method access) and `field_access` (private/package field access) with an explicit receiver.
pub fn visibility_errors_in(
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
                check_access(n, &root, source, bytes, symbols, resolver, cache, true, &mut out);
            }
            "field_access" => {
                check_access(n, &root, source, bytes, symbols, resolver, cache, false, &mut out);
            }
            _ => {}
        }
    }
    out
}

/// One `receiver.member` site. `is_method` picks the CST field names and the members list to search.
#[allow(clippy::too_many_arguments)]
fn check_access(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    is_method: bool,
    out: &mut Vec<Diagnostic>,
) {
    // `method_invocation` uses `object`/`name`; `field_access` uses `object`/`field`.
    let obj_field = "object";
    let name_field = if is_method { "name" } else { "field" };
    let Some(obj) = n.child_by_field_name(obj_field) else { return }; // bare member → no receiver → SKIP
    let Some(name_node) = n.child_by_field_name(name_field) else { return };
    if name_node.has_error() {
        return;
    }
    let Ok(member_name) = name_node.utf8_text(bytes) else { return };

    // Resolve the receiver's declaring type: either a VALUE receiver whose type we infer, or a
    // `Type.staticMember` receiver whose identifier resolves to a type. Either way we need a binary name.
    let Some(recv_binary) = receiver_type_binary(root, source, symbols, &obj, resolver, cache, bytes)
    else {
        return; // receiver type unknown → SKIP
    };

    // Never police JDK/library visibility (module exports & internal-API nuances are out of scope).
    if is_jdk_type(&recv_binary) {
        return;
    }
    // Only police the PROJECT'S OWN types: a dependency-jar receiver's accessibility (generated
    // accessors, split-package legacy frameworks) isn't soundly decidable from bytecode — same reason
    // as the JDK. Cheap early skip; the declaring-type guard below also covers a project subclass that
    // inherits a member from a dependency base.
    if !resolver.is_project_type(&recv_binary) {
        return;
    }

    // The hierarchy must be FULLY known: an unknown supertype could declare a public member that
    // shadows the one we found, so any gap means we can't be sure the access is illegal → SKIP.
    if !hierarchy_fully_known(resolver, &recv_binary) {
        return;
    }

    // Find the member UNAMBIGUOUSLY across the (fully-known) hierarchy: it must resolve to exactly one
    // declaration. Two matches (e.g. an override + the overridden) → ambiguous visibility → SKIP.
    let Some((declaring_binary, member)) =
        resolve_unique_member(resolver, &recv_binary, member_name, is_method)
    else {
        return; // absent, or ambiguous → SKIP (absence is the unknown-member check's job, not ours)
    };

    // The member's true OWNER must be a project type: a member inherited from a dependency/JDK base
    // (reached through a project receiver) has accessibility we don't model → SKIP (mirrors the JDK
    // and dependency-receiver guards above).
    if !resolver.is_project_type(&declaring_binary) {
        return;
    }

    // An INTERFACE (or `@interface`) member is implicitly PUBLIC (JLS §9.4) — a method with no explicit
    // access modifier, and every field (constant). So a call like `interfaceField.method()` must NEVER
    // be flagged as package/private access, whatever visibility the index happened to record for it.
    // (Interface `private` helper methods exist since Java 9, but they're callable only within the
    // interface — flagging an external call to one is a rare, low-value case not worth a false positive
    // risk, so we skip the whole interface here.) This is the fix for "methods on an interface-typed
    // field wrongly reported as not accessible".
    if resolver.members_of(&declaring_binary).is_some_and(|cm| cm.flags.is_interface || cm.flags.is_annotation) {
        return;
    }

    // The OUTERMOST type enclosing the access (lexical, authoritative). Used by BOTH visibility cases
    // to decide "same top-level type" — a class and all its nested types form one nest that shares
    // private access AND, trivially, one package.
    let access_top = enclosing_top_level_binary(n, bytes, symbols);

    match member.visibility {
        Visibility::Private => {
            // CASE 1: legal iff the access sits inside the declaring member's TOP-LEVEL type (the JLS
            // same-nest rule: an outer class and its nested types touch each other's privates).
            let Some(access_top) = access_top else {
                return; // can't identify the enclosing top-level type → SKIP
            };
            if in_same_nest(&access_top, &declaring_binary) {
                return; // same top-level type (outer ↔ nested private access) → legal
            }
            out.push(CheckId::InaccessibleMember.span(
                name_node.start_byte(),
                name_node.end_byte(),
                format!("`{member_name}` has private access in `{}`", simple_name(&declaring_binary)),
            ));
        }
        Visibility::Package => {
            // Same top-level type ⇒ same package ⇒ always legal — settle it before the package compare,
            // which mis-derives a NESTED type's package (its binary uses `/` for nesting too).
            if access_top.as_deref().is_some_and(|t| in_same_nest(t, &declaring_binary)) {
                return;
            }
            // CASE 2: legal iff the declaring type lives in the accessing file's package.
            let Some(access_pkg) = symbols.package.as_deref() else {
                return; // accessing file's package unknown → SKIP
            };
            // Normalise the accessing package to slash form for a like-for-like compare.
            let access_pkg = access_pkg.replace('.', "/");
            // A **prefix** test, not an equality one, and the reason is that a project-source NESTED
            // type's binary joins its nesting with `/` — the same separator as a package (the FQN is
            // built dotted, `Outer.Inner`, then slashed). So `com/acme/Outer/Inner` has no recoverable
            // package: comparing "everything before the last `/`" yielded `com/acme/Outer`, which can
            // never equal a real package, and EVERY same-package access to a package-visible member of
            // a nested type was reported inaccessible.
            //
            // Telling a nested class apart from a sub-package inside a slash-joined binary needs a
            // case convention (`Outer` vs `other`), and a check that guesses there would start
            // flagging correct code the moment a project spells a name unusually. So the rule is
            // "declared under the accessing package", which is exact for a nested type and
            // deliberately lenient for a genuine SUB-package: `com/acme` accessing `com/acme/sub/Foo`
            // is no longer flagged. That is an under-report in a narrow slice, and this check's stated
            // policy is that under-reporting is acceptable where a wrong diagnostic is not.
            if declaring_binary.starts_with(&format!("{access_pkg}/")) {
                return; // same package (or a sub-package we decline to judge) → legal
            }
            if package_of_binary(&declaring_binary).is_none() {
                return; // declaring type is in the default (root) package / path-less → SKIP
            }
            out.push(CheckId::InaccessibleMember.span(
                name_node.start_byte(),
                name_node.end_byte(),
                format!(
                    "`{member_name}` is not public in `{}` and can't be accessed from package `{}`",
                    simple_name(&declaring_binary),
                    access_pkg
                ),
            ));
        }
        // Public / Protected are never flagged (protected subclass rules are subtle → skipped whole).
        Visibility::Public | Visibility::Protected => {}
    }
}

/// The binary name of the receiver's declaring type, for BOTH shapes we handle:
///   * a **value** receiver (`local.member`, `foo().member`) → inferred static type;
///   * a **`Type.staticMember`** receiver — inference yields no value type, so we fall back to
///     resolving the receiver's *written text* as a type name (only when it's a plain identifier /
///     dotted type reference, i.e. a `field_access`/`identifier` object node).
/// `None` when neither yields a resolvable type → the caller SKIPs.
#[allow(clippy::too_many_arguments)]
fn receiver_type_binary(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    obj: &Node,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    bytes: &[u8],
) -> Option<String> {
    // Value receiver: inferred static type wins (covers locals, fields, chains, generics).
    if let Some(ty) = infer_node_type_cached(root, source, symbols, obj, resolver, cache) {
        if !ty.binary_name.is_empty() {
            return Some(ty.binary_name);
        }
    }
    // Static receiver: `Type.staticMember`. Only a plain `identifier` (or dotted type ref) can be a
    // bare type name; anything else (a call result, a cast, …) would have inferred above.
    if matches!(obj.kind(), "identifier" | "scoped_identifier" | "field_access") {
        if let Ok(text) = obj.utf8_text(bytes) {
            // `type_binary` resolves via same-file decls, imports, then the resolver; only accept a
            // result the resolver actually knows the members of (an arbitrary dotted path resolves to a
            // binary shape but may be a package prefix → not a real type).
            if let Some(bin) = type_binary(text, symbols, resolver) {
                if resolver.members_of(&bin).is_some() {
                    return Some(bin);
                }
            }
        }
    }
    None
}

/// Find the member `name` (a method when `is_method`, else a field) across `binary`'s fully-known
/// hierarchy, returning `(declaring_binary, member)` iff it resolves to exactly ONE declaring TYPE.
/// Within that type, overloads of `name` collapse to their MOST VISIBLE representative (a call binds
/// to one overload by argument types, which we don't resolve — so if any overload is accessible we
/// must not flag). `None` when absent OR the name is declared in TWO different supertypes (an
/// override / hide) — either way the caller SKIPs. Requiring a unique declaring type is the
/// conservative core: across levels we can't soundly pick which declaration the call binds to.
fn resolve_unique_member(
    resolver: &dyn TypeResolver,
    binary: &str,
    name: &str,
    is_method: bool,
) -> Option<(String, Member)> {
    let mut found: Option<(String, Member)> = None;
    let mut ambiguous = false;
    for_each_supertype(resolver, binary, &mut |bn, cm| {
        let list = if is_method { &cm.methods } else { &cm.fields };
        // The MOST VISIBLE overload of `name` in THIS type is the representative. A call binds to
        // exactly ONE overload (chosen by argument types, which we don't resolve), so picking the
        // most-visible one is the only sound choice: if ANY overload is accessible (public/protected),
        // the call might bind to it, and we must NOT flag. (The old code took the FIRST-declared
        // overload and assumed a shared visibility — wrong: `private foo(int)` + `public foo(String)`
        // made `x.foo("s")` a false "private access".) Fields have no overloads, so this is just it.
        let mut best: Option<&Member> = None;
        for m in list {
            if m.name == name {
                best = match best {
                    Some(prev) if visibility_rank(prev.visibility) >= visibility_rank(m.visibility) => {
                        Some(prev)
                    }
                    _ => Some(m),
                };
            }
        }
        let declared_here = best.is_some();
        if let Some(m) = best {
            if found.is_none() {
                found = Some((bn.to_string(), m.clone()));
            }
        }
        // The SAME name declared in a DIFFERENT supertype (an override / hide) makes the binding
        // ambiguous for a visibility verdict → bail.
        if declared_here {
            if let Some((owner, _)) = &found {
                if owner != bn {
                    ambiguous = true;
                }
            }
        }
    });
    if ambiguous {
        return None;
    }
    found
}

/// Whether `binary` is a JDK / platform / common-library type whose visibility we deliberately don't
/// police (module exports, internal APIs, and reflection make an AST-only verdict unsound here).
fn is_jdk_type(binary: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "java/", "javax/", "jakarta/", "jdk/", "sun/", "com/sun/", "org/w3c/", "org/xml/",
        "org/omg/", "org/ietf/",
    ];
    PREFIXES.iter().any(|p| binary.starts_with(p))
}

/// Higher = more visible (`Public` 3 → `Private` 0). Used to pick the most-visible overload of a
/// name as the accessibility representative: a call binds to one overload by its argument types
/// (which this check doesn't resolve), so only the most-visible one is a sound basis for a verdict —
/// if any overload is accessible, we must not flag.
fn visibility_rank(v: Visibility) -> u8 {
    match v {
        Visibility::Public => 3,
        Visibility::Protected => 2,
        Visibility::Package => 1,
        Visibility::Private => 0,
    }
}

/// Whether the member's declaring type belongs to the SAME top-level type as `access_top` (the
/// outermost type enclosing the access) — the JLS "nest" that shares private access. True when the
/// declaring binary IS `access_top` or is a type nested inside it. Bytecode spells nesting with `$`
/// (`Outer$Inner`); a PROJECT source type's binary comes from its dotted FQN, so nesting shows up as
/// `/` (`com/acme/Outer/Inner`) — indistinguishable from a package boundary. Accepting the `/` form
/// can therefore only ever SKIP a real error (a false negative), never invent one — the correct bias
/// for this never-false-positive check, and the fix for the outer-class-touches-nested-private case.
fn in_same_nest(access_top: &str, declaring_binary: &str) -> bool {
    declaring_binary == access_top
        || declaring_binary.starts_with(&format!("{access_top}$"))
        || declaring_binary.starts_with(&format!("{access_top}/"))
}

/// The TOP-LEVEL binary name of a (possibly nested) declaring type: everything before the first `$`.
/// `com/acme/Outer$Inner` → `com/acme/Outer`; a top-level `com/acme/Foo` is returned unchanged. Used
/// so a nested class accessing its OUTER's privates is treated as inside the same top-level type.
fn top_level_binary(binary: &str) -> &str {
    binary.split('$').next().unwrap_or(binary)
}

/// The package path (slash form, e.g. `com/acme/other`) of a binary name, or `None` when the type is
/// in the default (root) package — i.e. the binary has no `/` before its (possibly nested) class part.
/// `None` is treated by the caller as "unknown package" → SKIP (conservative).
fn package_of_binary(binary: &str) -> Option<String> {
    // Strip any nested-class suffix first: the package is derived from the OUTER class's path.
    let top = top_level_binary(binary);
    let idx = top.rfind('/')?;
    if idx == 0 {
        return None;
    }
    Some(top[..idx].to_string())
}

/// The binary name of the OUTERMOST (top-level) type enclosing `n` — the type whose privates the
/// access is lexically allowed to touch. Walks parents recording the last type declaration seen, then
/// prefixes the accessing file's package. `None` when `n` isn't inside any named type declaration.
fn enclosing_top_level_binary(n: Node, bytes: &[u8], symbols: &FileSymbols) -> Option<String> {
    let mut top_name: Option<String> = None;
    let mut cur = n.parent();
    while let Some(p) = cur {
        if matches!(
            p.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            if let Some(name) = p.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) {
                top_name = Some(name.to_string()); // keep overwriting → the LAST (outermost) wins
            }
        }
        cur = p.parent();
    }
    let name = top_name?;
    // Prefer the extracted TypeDecl's FQN (authoritative for this file); else compose package + name.
    if let Some(td) = symbols.types.iter().find(|t| t.name == name) {
        // Use only the top-level segment of the FQN (a nested TypeDecl's fqn could carry a `.Inner`).
        let fqn_binary = td.fqn.replace('.', "/");
        return Some(top_level_binary(&fqn_binary).to_string());
    }
    match symbols.package.as_deref() {
        Some(pkg) if !pkg.is_empty() => Some(format!("{}/{name}", pkg.replace('.', "/"))),
        _ => Some(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tree_sitter::Parser;

    /// The same fixed resolver shape the members/fields tests use: `binary → members` + `simple → binary`.
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
        // Everything under `com/dep/` stands in for a DEPENDENCY jar type (not project source); the
        // rest is treated as the user's own project code. Lets the tests exercise the exemption.
        fn is_project_type(&self, binary: &str) -> bool {
            !binary.starts_with("com/dep/")
        }
    }

    fn field(name: &str, ty: &str, vis: Visibility) -> Member {
        Member::field(name, TypeRef::simple(ty.to_string())).vis(vis)
    }
    fn method(name: &str, vis: Visibility) -> Member {
        Member::method(name, TypeRef::simple("void".to_string()), Vec::new()).vis(vis)
    }

    /// `com/acme/other/OtherPackageClass` (super Object) with the four visibilities, plus a String
    /// (JDK) with a `private` field to prove JDK receivers are never policed.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/other/OtherPackageClass".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![
                    method("private_method", Visibility::Private),
                    // `overloaded` has BOTH a private and a public overload — a call binds to one by
                    // argument types, so the most-visible (public) representative must win.
                    method("overloaded", Visibility::Private),
                    method("overloaded", Visibility::Public),
                ],
                fields: vec![
                    field("secret_value", "int", Visibility::Private),
                    field("package_value", "int", Visibility::Package),
                    field("prot_value", "int", Visibility::Protected),
                    field("ok", "int", Visibility::Public),
                ],
                flags: Default::default(),
            },
        );
        // A DEPENDENCY-jar type (under `com/dep/`) with a package-private field — must never be
        // flagged, even from another package, because we don't police library visibility.
        members.insert(
            "com/dep/LibClass".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("lib_secret", "int", Visibility::Package)],
                flags: Default::default(),
            },
        );
        // A NESTED project type: `Outer.Inner` → binary `com/acme/access/Outer/Inner` (dotted FQN →
        // slash, so nesting looks like a package segment). Its `private` field is legally reachable
        // from the enclosing `Outer` (same nest) — the case the `$`-only top-level split used to miss.
        members.insert(
            "com/acme/access/Outer/Inner".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![
                    field("secret", "int", Visibility::Private),
                    // …and a PACKAGE-visible one: reachable from any class in `com.acme.access`,
                    // which the old "everything before the last `/`" package derivation could never
                    // conclude (it read the package as `com/acme/access/Outer`).
                    field("shared", "int", Visibility::Package),
                ],
                flags: Default::default(),
            },
        );
        // A PROJECT interface in another package whose method got recorded with `Package` visibility
        // (the indexing quirk this guards against). Interface members are implicitly public, so a call
        // through an interface-typed field must never be flagged.
        members.insert(
            "com/acme/other/SomeService".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![method("serve", Visibility::Package)],
                fields: Vec::new(),
                flags: bennu_java::prelude::ClassFlags { is_interface: true, ..Default::default() },
            },
        );
        // A JDK type with a private field — must never be flagged.
        members.insert(
            "java/lang/String".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("hash", "int", Visibility::Private)],
                flags: Default::default(),
            },
        );
        let simple = [
            ("OtherPackageClass", "com/acme/other/OtherPackageClass"),
            ("SomeService", "com/acme/other/SomeService"),
            ("LibClass", "com/dep/LibClass"),
            ("Inner", "com/acme/access/Outer/Inner"),
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Run the check over a full source string (with its own package/imports/classes) and return the
    /// diagnostic messages. The accessing file lives in package `com.acme.access`.
    fn diags(src: &str) -> Vec<String> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let symbols = bennu_java::prelude::extract_symbols(src);
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        visibility_errors_in(root, &nodes, src, &symbols, &resolver(), &InferCache::new())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    /// An accessing file in `com.acme.access` with a body that references `other` (an OtherPackageClass).
    fn access(body: &str) -> String {
        format!(
            "package com.acme.access;\nclass Accessor {{ void m(com.acme.other.OtherPackageClass other) {{ {body} }} }}"
        )
    }

    #[test]
    fn private_field_from_other_class_is_flagged() {
        let d = diags(&access("int a = other.secret_value;"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("secret_value") && d[0].contains("private access"), "{d:?}");
        assert!(d[0].contains("OtherPackageClass"), "{d:?}");
    }

    #[test]
    fn private_method_from_other_class_is_flagged() {
        let d = diags(&access("other.private_method();"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("private_method") && d[0].contains("private access"), "{d:?}");
    }

    #[test]
    fn package_field_from_other_package_is_flagged() {
        let d = diags(&access("int a = other.package_value;"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("package_value") && d[0].contains("not public"), "{d:?}");
        assert!(d[0].contains("com/acme/access"), "{d:?}");
    }

    #[test]
    fn overloaded_method_with_a_public_overload_is_not_flagged() {
        // `overloaded` has a private AND a public overload; the call binds to one by argument types
        // (not resolved here), so the most-visible overload is the representative → never flagged.
        let d = diags(&access("other.overloaded();"));
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn public_field_is_not_flagged() {
        assert!(diags(&access("int a = other.ok;")).is_empty());
    }

    #[test]
    fn interface_member_is_never_flagged_even_if_recorded_package() {
        // Calling a method through an INTERFACE-typed field must not be flagged as package/private
        // access — interface members are implicitly public (JLS §9.4), whatever the index recorded.
        let src = "package com.acme.access;\nclass Accessor { void m(com.acme.other.SomeService svc) { svc.serve(); } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn protected_field_is_not_flagged() {
        // Protected subclass rules are subtle → we never flag protected.
        assert!(diags(&access("int a = other.prot_value;")).is_empty());
    }

    #[test]
    fn private_field_from_within_declaring_class_is_not_flagged() {
        // The access sits INSIDE OtherPackageClass's own top-level type → legal → SKIP. The receiver
        // `other` is a value of the declaring type, and the enclosing top-level type IS the declaring
        // type, so the top-level identities match. Uses a same-type parameter (proven to infer above).
        let src = "package com.acme.other;\nclass OtherPackageClass { int r(com.acme.other.OtherPackageClass other) { return other.secret_value; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn package_field_from_same_package_is_not_flagged() {
        // Accessing file also in `com.acme.other` → same package → legal.
        let src = "package com.acme.other;\nclass Same { void m(com.acme.other.OtherPackageClass other) { int a = other.package_value; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    /// The reported bug: a package-visible member of a **nested** type, read from a SIBLING class in
    /// the same package. A project nested type's binary joins its nesting with `/`
    /// (`com/acme/access/Outer/Inner`), so deriving "the package" as everything before the last `/`
    /// gave `com/acme/access/Outer` — which can never equal a real package, and every such access was
    /// reported inaccessible even though it is legal.
    #[test]
    fn package_member_of_a_nested_type_is_reachable_from_the_same_package() {
        // `Inner` resolves through the simple-name map, exactly as the sibling nest test does.
        let src = "package com.acme.access;\nclass Sibling { int m(Inner i) { return i.shared; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    /// The private member of that same nested type IS still flagged from a sibling — the fix widened
    /// the *package* rule, not the private one.
    #[test]
    fn private_member_of_a_nested_type_is_still_flagged_from_a_sibling() {
        let src = "package com.acme.access;\nclass Sibling { int m(Inner i) { return i.secret; } }";
        let d = diags(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("private access"), "{d:?}");
    }

    /// And a package member in a genuinely DIFFERENT package is still flagged — the prefix rule only
    /// relaxes what sits *under* the accessing package.
    #[test]
    fn package_field_from_an_unrelated_package_is_still_flagged() {
        let src = "package com.acme.access;\n\
                   class A { void m(com.acme.other.OtherPackageClass o) { int a = o.package_value; } }";
        let d = diags(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("not public"), "{d:?}");
    }

    #[test]
    fn unresolvable_receiver_is_not_flagged() {
        // `mystery`'s type is unknown to the resolver → inference yields nothing → SKIP.
        let src = "package com.acme.access;\nclass A { void m(Unknown mystery) { Object o = mystery.secret_value; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn outer_accessing_nested_private_is_not_flagged() {
        // A class and its nested types share one nest → the outer legally reads the nested type's
        // `private` field. The nested project binary uses `/` for nesting (`.../Outer/Inner`), which
        // the same-nest check now recognises (the `$`-only split used to make this a false positive).
        let src = "package com.acme.access;\nclass Outer { static class Inner { private int secret; } int m(Inner i) { return i.secret; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn dependency_receiver_is_not_flagged() {
        // `LibClass` (a `com/dep/` dependency type) has a package-private field, accessed from another
        // package. A real cross-package access, but we don't police LIBRARY visibility → SKIP. This is
        // the false positive dependency indexing surfaced on legacy Struts/Entando types.
        let src = "package com.acme.access;\nclass A { void m(com.dep.LibClass lib) { int x = lib.lib_secret; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn jdk_receiver_is_not_flagged() {
        // `String` has a private `hash` field in our resolver, but JDK receivers are never policed.
        let src = "package com.acme.access;\nclass A { void m(String s) { int h = s.hash; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn bare_member_without_receiver_is_not_flagged() {
        // No explicit receiver → not a `receiver.member` access → SKIP.
        let src = "package com.acme.access;\nclass A { int secret_value; int m() { return secret_value; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }
}
