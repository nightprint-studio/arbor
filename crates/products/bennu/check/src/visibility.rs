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
//!      type's package differs from the accessing file's package.
//!
//! ## Never a false positive (this is accessibility — when unsure, SKIP)
//! Every gate below is a SKIP, not a flag:
//!   * no explicit receiver (bare `member`) → SKIP;
//!   * receiver type doesn't infer to a resolvable type (value receiver) / `Type` doesn't resolve
//!     (static receiver) → SKIP;
//!   * receiver type is a JDK/library type (`java/…`, `javax/…`, …) → SKIP (we don't police JDK
//!     visibility — too many nuances: module exports, `@jdk.internal`, etc.);
//!   * the member doesn't resolve UNAMBIGUOUSLY to exactly one declaration across a FULLY-KNOWN
//!     hierarchy → SKIP (an unknown supertype might declare a public one that shadows it);
//!   * the resolved member is `Public` or `Protected` → never flagged (protected subclass rules are
//!     subtle → skipped entirely);
//!   * CASE 1: the access IS inside the declaring top-level type → legal → SKIP;
//!   * CASE 2: the two packages are equal, or either package is unknown → SKIP.

use bennu_java::prelude::{
    infer_node_type_cached, FileSymbols, InferCache, Member, TypeResolver, Visibility,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::members::simple_name;
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

    match member.visibility {
        Visibility::Private => {
            // CASE 1: legal iff the access is lexically inside the declaring member's TOP-LEVEL type.
            let declaring_top = top_level_binary(&declaring_binary);
            let Some(access_top) = enclosing_top_level_binary(n, bytes, symbols) else {
                return; // can't identify the enclosing top-level type → SKIP
            };
            if access_top == declaring_top {
                return; // same top-level type (possibly a nested class touching the outer's privates) → legal
            }
            out.push(Diagnostic {
                message: format!(
                    "`{member_name}` has private access in `{}`",
                    simple_name(&declaring_binary)
                ),
                severity: "error".to_string(),
                start: name_node.start_byte(),
                end: name_node.end_byte(),
            });
        }
        Visibility::Package => {
            // CASE 2: legal iff the accessing file's package equals the declaring type's package.
            // Both must be KNOWN; either unknown → SKIP.
            let Some(access_pkg) = symbols.package.as_deref() else {
                return; // accessing file's package unknown → SKIP
            };
            let Some(decl_pkg) = package_of_binary(&declaring_binary) else {
                return; // declaring type is in the default (root) package or path-less → treat as unknown → SKIP
            };
            // Normalise the accessing package to slash form for a like-for-like compare.
            let access_pkg = access_pkg.replace('.', "/");
            if access_pkg == decl_pkg {
                return; // same package → legal
            }
            out.push(Diagnostic {
                message: format!(
                    "`{member_name}` is not public in `{}` and can't be accessed from package `{}`",
                    simple_name(&declaring_binary),
                    access_pkg
                ),
                severity: "error".to_string(),
                start: name_node.start_byte(),
                end: name_node.end_byte(),
            });
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
/// hierarchy, returning `(declaring_binary, member)` iff it resolves to EXACTLY ONE declaration.
/// `None` when absent OR ambiguous (two matches) — either way the caller SKIPs. Requiring uniqueness
/// is the conservative core: if the same name is declared at two levels with different visibility
/// (an override widening access), we can't soundly pick which one the call binds to → SKIP.
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
        // An explicit loop, never `.find`/`.any` on the members slice per the visibility of overloads:
        // we want to know if MORE than one supertype declares the name (any overload set at one level
        // counts once — same declaring type, so still unambiguous for a private/package decision).
        let mut declared_here = false;
        for m in list {
            if m.name == name {
                declared_here = true;
                // Keep the FIRST-seen member of this declaring type (overloads share visibility scope
                // for our purposes — a private overload set is private; we only branch on visibility).
                if found.is_none() {
                    found = Some((bn.to_string(), m.clone()));
                }
                break;
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
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![method("private_method", Visibility::Private)],
                fields: vec![
                    field("secret_value", "int", Visibility::Private),
                    field("package_value", "int", Visibility::Package),
                    field("prot_value", "int", Visibility::Protected),
                    field("ok", "int", Visibility::Public),
                ],
                flags: Default::default(),
            },
        );
        // A JDK type with a private field — must never be flagged.
        members.insert(
            "java/lang/String".to_string(),
            ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("hash", "int", Visibility::Private)],
                flags: Default::default(),
            },
        );
        let simple = [
            ("OtherPackageClass", "com/acme/other/OtherPackageClass"),
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
    fn public_field_is_not_flagged() {
        assert!(diags(&access("int a = other.ok;")).is_empty());
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

    #[test]
    fn unresolvable_receiver_is_not_flagged() {
        // `mystery`'s type is unknown to the resolver → inference yields nothing → SKIP.
        let src = "package com.acme.access;\nclass A { void m(Unknown mystery) { Object o = mystery.secret_value; } }";
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
