//! An override that makes a method **less visible** than the one it overrides —
//! `compiler.err.override.weaker.access`.
//!
//! Java forbids it (JLS §8.4.8.3) because the supertype's contract promises the method is callable:
//! narrowing it would let a subtype withdraw a promise its supertype already made, and every caller
//! holding a supertype reference would still call it. It is easy to write by accident — an IDE's
//! "implement methods" writes what the cursor's class uses, and a `protected` slips in where the
//! interface said `public`.
//!
//! **Only widening from `public` or `protected` is judged.** Package-private is deliberately left
//! alone: whether it is even inherited depends on the two types being in the same package, which
//! this check would have to be sure of to say anything — and a wrong answer here accuses code that
//! compiles. Public and protected are inherited everywhere, so their narrowing is unambiguous.
//!
//! An interface method is implicitly `public`, which is where most of these live: writing
//! `protected void run()` against `Runnable` is a compile error the editor should not make you
//! discover from Maven.

use std::collections::HashMap;

use bennu_java::prelude::{FileSymbols, MemberKind, TypeResolver, Visibility};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::method_sig::method_param_binaries;
use crate::nodes::{has_keyword, text};
use crate::walk::for_each_supertype;

/// Every narrowing override in the pre-collected node slice.
pub fn override_access_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if matches!(n.kind(), "class_declaration" | "enum_declaration" | "record_declaration") {
            check_type(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_type(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(body) = n.child_by_field_name("body") else { return };

    let supers = crate::supertypes::binaries(n, bytes, symbols, resolver);
    if supers.is_empty() {
        return;
    }

    // name → (erased parameter types, the visibility the supertype promised). Only `public` and
    // `protected` are collected; see the module doc for why package-private is not judged.
    let mut promised: HashMap<String, Vec<(Vec<String>, Visibility)>> = HashMap::new();
    for sup in &supers {
        for_each_supertype(resolver, sup, &mut |_bn, cm| {
            for m in &cm.methods {
                let inherited = m.kind == MemberKind::Method
                    && !m.is_static
                    && matches!(m.visibility, Visibility::Public | Visibility::Protected)
                    && m.name != "<init>"
                    && m.name != "<clinit>";
                if inherited {
                    let params = m.params.iter().map(|p| p.binary_name.clone()).collect();
                    promised.entry(m.name.clone()).or_default().push((params, m.visibility));
                }
            }
        });
    }
    if promised.is_empty() {
        return;
    }

    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "method_declaration" || has_keyword(m, bytes, "static") {
            continue;
        }
        let Some(name_node) = m.child_by_field_name("name") else { continue };
        let Some(name) = text(name_node, bytes) else { continue };
        let Some(candidates) = promised.get(&name) else { continue };
        let Some(params) = method_param_binaries(m, bytes, symbols, resolver) else { continue };

        // The widest promise this signature has to keep: a method can inherit the same signature
        // from a class and an interface, and it is the more visible of the two that binds.
        let Some(required) = candidates
            .iter()
            .filter(|(p, _)| *p == params)
            .map(|(_, v)| *v)
            .max_by_key(|v| rank(*v))
        else {
            continue;
        };
        let declared = declared_visibility(m, bytes);
        if rank(declared) < rank(required) {
            out.push(CheckId::WeakerAccessOverride.at(
                name_node,
                format!(
                    "`{name}` is {} here but {} in the type it overrides — an override cannot \
                     reduce visibility",
                    word(declared),
                    word(required)
                ),
            ));
        }
    }
}

/// How visible each modifier is, most restrictive first. Only an ordering — the numbers mean
/// nothing on their own.
fn rank(v: Visibility) -> u8 {
    match v {
        Visibility::Private => 0,
        Visibility::Package => 1,
        Visibility::Protected => 2,
        Visibility::Public => 3,
    }
}

fn word(v: Visibility) -> &'static str {
    match v {
        Visibility::Private => "private",
        Visibility::Package => "package-private",
        Visibility::Protected => "protected",
        Visibility::Public => "public",
    }
}

/// The visibility a method declares. No keyword means package-private — except in an interface,
/// where it means `public`; but an interface's own methods are never the overrider here (this is
/// only called for members of a class, enum or record), so the plain rule holds.
fn declared_visibility(m: Node, bytes: &[u8]) -> Visibility {
    if has_keyword(m, bytes, "public") {
        Visibility::Public
    } else if has_keyword(m, bytes, "protected") {
        Visibility::Protected
    } else if has_keyword(m, bytes, "private") {
        Visibility::Private
    } else {
        Visibility::Package
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
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

    fn method(name: &str, visibility: Visibility) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple("void"),
            params: Vec::new(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility,
            raw_signature: String::new(),
            throws: Vec::new(),
            annotations: Vec::new(),
        }
    }

    fn ty(is_interface: bool, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: (!is_interface).then(|| "java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags { is_interface, ..ClassFlags::default() },
        }
    }

    /// `Runnable.run()` is public (an interface method always is); `Sup` promises `public greet()`
    /// and `protected helper()`; `Pkg` has a package-private one, which is never judged.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), ty(false, Vec::new()));
        members.insert(
            "java/lang/Runnable".into(),
            ty(true, vec![method("run", Visibility::Public)]),
        );
        members.insert(
            "com/acme/Sup".into(),
            ty(
                false,
                vec![
                    method("greet", Visibility::Public),
                    method("helper", Visibility::Protected),
                ],
            ),
        );
        members.insert(
            "com/acme/Pkg".into(),
            ty(false, vec![method("hidden", Visibility::Package)]),
        );
        let simple = [
            ("Runnable", "java/lang/Runnable"),
            ("Sup", "com/acme/Sup"),
            ("Pkg", "com/acme/Pkg"),
            ("Object", "java/lang/Object"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = bennu_java::prelude::extract_symbols_from_root(&root, src);
        override_access_errors_in(&nodes, src, &symbols, &resolver())
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn an_interface_method_implemented_as_protected_is_flagged() {
        let src = "class A implements Runnable { protected void run() {} }";
        assert_eq!(codes(src), ["weaker-access-override"]);
    }

    #[test]
    fn an_interface_method_implemented_with_no_modifier_is_flagged() {
        let src = "class A implements Runnable { void run() {} }";
        assert_eq!(codes(src), ["weaker-access-override"]);
    }

    #[test]
    fn implementing_it_public_is_fine() {
        assert!(codes("class A implements Runnable { public void run() {} }").is_empty());
    }

    #[test]
    fn narrowing_a_superclass_public_method_is_flagged() {
        assert_eq!(codes("class A extends Sup { protected void greet() {} }"), ["weaker-access-override"]);
    }

    #[test]
    fn widening_a_protected_method_to_public_is_fine() {
        assert!(codes("class A extends Sup { public void helper() {} }").is_empty());
    }

    #[test]
    fn keeping_protected_protected_is_fine() {
        assert!(codes("class A extends Sup { protected void helper() {} }").is_empty());
    }

    /// Package-private is never judged: whether it is inherited at all depends on the packages, and
    /// this check does not know them.
    #[test]
    fn a_package_private_supertype_method_is_never_judged() {
        assert!(codes("class A extends Pkg { private void hidden() {} }").is_empty());
    }

    /// A static method with the same name hides rather than overrides, so the rule does not apply.
    #[test]
    fn a_static_method_of_the_same_name_is_not_an_override() {
        assert!(codes("class A extends Sup { private static void greet() {} }").is_empty());
    }

    /// A different signature is a different method — an overload, not an override.
    #[test]
    fn a_different_signature_is_an_overload_not_an_override() {
        assert!(codes("class A implements Runnable { protected void run(int n) {} public void run() {} }").is_empty());
    }

    #[test]
    fn a_type_with_no_supertype_is_left_alone() {
        assert!(codes("class A { private void run() {} }").is_empty());
    }
}
