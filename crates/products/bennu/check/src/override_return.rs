//! Incompatible override return-type diagnostics (resolver-backed).
//!
//! When a method overrides a supertype method, its return type must be either the SAME type or a
//! subtype (covariant return, JLS §8.4.8.3). A method that matches an inherited method by name **and**
//! erased parameter types but returns an UNRELATED reference type — `String get()` overriding
//! `Number get()` — is a compile error ("return type is incompatible with …").
//!
//! This is the case [`crate::inherit_cycle`]'s `@Override`-overrides-nothing check deliberately leaves
//! alone: there, a name match of any arity is treated as "might override" and SKIPPED, precisely so a
//! legal covariant override is never mis-flagged. Here we take the opposite, equally-conservative
//! stance — we flag ONLY a signature that DEFINITELY overrides (same name + same erased params, the
//! [`crate::finals`] `final_override` matching) yet whose return type DEFINITELY isn't covariant.
//!
//! PARAMOUNT — never a false positive. We flag ONLY when EVERY guard holds:
//!   * the method matches a supertype method by name + erased parameter types (a real override, never
//!     an overload);
//!   * BOTH return types resolve to concrete reference classes with FULLY-KNOWN hierarchies (a
//!     primitive/`void`, a type variable, an array, or any unresolved return → SKIP);
//!   * the overriding return type does NOT reach (isn't a subtype of) the overridden return type.
//! Anything uncertain — generics erased to a shared bound, an un-indexed supertype, a widening we
//! can't confirm — is skipped rather than risk a wrong report.

use std::collections::HashMap;

use bennu_java::prelude::{FileSymbols, MemberKind, TypeResolver, Visibility};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known, reaches};

/// Flag every method whose return type is an illegal (non-covariant) override of an inherited method.
pub fn override_return_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if matches!(n.kind(), "class_declaration" | "enum_declaration") {
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

    // Supertypes to scan: the explicit `extends` (if resolvable) + every `implements` interface. We do
    // NOT seed `java/lang/Object` — its methods (`toString`/`equals`/…) are the covariant-legal cases a
    // real override wants; only a declared supertype gives us a return type worth comparing.
    let mut supers: Vec<String> = Vec::new();
    if let Some(ext) = superclass_text(n, bytes) {
        if let Some(bin) = type_binary(&ext, symbols, resolver) {
            supers.push(bin);
        }
    }
    for iface in implements_texts(n, bytes) {
        if let Some(bin) = type_binary(&iface, symbols, resolver) {
            supers.push(bin);
        }
    }
    if supers.is_empty() {
        return;
    }

    // name → the set of (erased params, return binary) of overridable supertype methods.
    let mut inherited: HashMap<String, Vec<(Vec<String>, String)>> = HashMap::new();
    for sup in &supers {
        for_each_supertype(resolver, sup, &mut |_bn, cm| {
            for m in &cm.methods {
                let overridable = m.kind == MemberKind::Method
                    && !m.is_static
                    && m.visibility != Visibility::Private
                    && m.name != "<init>"
                    && m.name != "<clinit>";
                if overridable {
                    let params = m.params.iter().map(|p| p.binary_name.clone()).collect();
                    inherited
                        .entry(m.name.clone())
                        .or_default()
                        .push((params, m.return_type.binary_name.clone()));
                }
            }
        });
    }
    if inherited.is_empty() {
        return;
    }

    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "method_declaration" {
            continue;
        }
        if has_keyword(m, bytes, "static") || has_keyword(m, bytes, "private") {
            continue; // static / private methods don't override
        }
        let Some(name_node) = m.child_by_field_name("name") else { continue };
        let Some(name) = text(name_node, bytes) else { continue };
        let Some(candidates) = inherited.get(&name) else { continue };
        let Some(params) = method_param_binaries(m, bytes, symbols, resolver) else { continue };

        // SKIP unless the overriding method's own return type resolves to a concrete reference class.
        let Some(sub_ret) = method_return_binary(m, bytes, symbols, resolver) else { continue };
        let Some(sub_ret) = concrete_ref(&sub_ret, resolver) else { continue };

        for (super_params, super_ret) in candidates {
            if *super_params != params {
                continue; // different signature → an overload, not this override
            }
            // SKIP unless the overridden return type is ALSO a concrete reference class we can reason
            // about. A type variable / primitive / array / unresolved super return → skip.
            let Some(super_ret) = concrete_ref(super_ret, resolver) else { continue };
            if super_ret == sub_ret {
                continue; // identical return → a legal (non-covariant) override
            }
            // Both fully known, and the sub return is NOT a subtype of the super return → illegal.
            if hierarchy_fully_known(resolver, &sub_ret)
                && hierarchy_fully_known(resolver, &super_ret)
                && !reaches(resolver, &sub_ret, &super_ret)
            {
                out.push(err(
                    format!(
                        "Return type `{}` is not compatible with the overridden method's `{}`",
                        simple_name(&sub_ret),
                        simple_name(&super_ret)
                    ),
                    name_node,
                ));
                break; // one report per method is enough
            }
        }
    }
}

/// The overriding method's return type as a concrete binary name, or `None` when it's `void`, a
/// primitive, or doesn't resolve. Read off the `type` field of the `method_declaration`.
fn method_return_binary(
    md: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    let ty = md.child_by_field_name("type")?;
    let text = ty.utf8_text(bytes).ok()?;
    type_binary(text, symbols, resolver)
}

/// Validate a binary name as a concrete reference class the resolver knows: not a primitive, `void`, a
/// single-letter type variable, or an array. `None` (→ SKIP) for any of those.
fn concrete_ref(binary: &str, resolver: &dyn TypeResolver) -> Option<String> {
    if is_primitive(binary) || binary.ends_with("[]") || is_type_var(binary) {
        return None;
    }
    resolver.members_of(binary)?;
    Some(binary.to_string())
}

fn is_primitive(binary: &str) -> bool {
    matches!(
        binary,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

fn is_type_var(binary: &str) -> bool {
    binary.len() == 1 && binary.chars().all(|c| c.is_ascii_uppercase())
}

/// The erased binary names of a method's parameter types. `None` (skip the method) if any parameter
/// type can't be resolved, or the method is varargs. Mirrors `finals::method_param_binaries`.
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
                out.push(type_binary(text, symbols, resolver)?);
            }
            "spread_parameter" => return None, // varargs — skip
            _ => {}
        }
    }
    Some(out)
}

/// The `extends` type text of a class (`superclass` wrapper), if any.
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

/// The `implements` interface type texts of a class/enum (`interfaces` → `type_list`).
fn implements_texts(n: Node, bytes: &[u8]) -> Vec<String> {
    let Some(w) = n.child_by_field_name("interfaces") else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![w];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "type_identifier" | "scoped_type_identifier" | "generic_type") {
            if let Some(t) = text(node, bytes) {
                out.push(t);
            }
            continue;
        }
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            stack.push(ch);
        }
    }
    out
}

fn has_keyword(node: Node, bytes: &[u8], keyword: &str) -> bool {
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

fn text(node: Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(str::to_string)
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic { message, severity: "error".to_string(), start: node.start_byte(), end: node.end_byte() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tree_sitter::Parser;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }
    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _i: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    /// A method returning `ret` (binary name), no params (or the given param binaries).
    fn method(name: &str, ret: &str, params: &[&str]) -> Member {
        let params = params.iter().map(|p| TypeRef::simple(p.to_string())).collect();
        Member::method(name, TypeRef::simple(ret.to_string()), params)
    }

    fn cls(superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            superclass: superclass.map(str::to_string),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// Object; `Number`; `String` and `Integer` (both subtypes of Object, `Integer` a subtype of
    /// `Number`); a `NumericBase` with `Number getValue()`; a `Producer` interface with `Object make()`.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cls(None, vec![]));
        members.insert("java/lang/Number".to_string(), cls(Some("java/lang/Object"), vec![]));
        members.insert("java/lang/String".to_string(), cls(Some("java/lang/Object"), vec![]));
        members.insert("java/lang/Integer".to_string(), cls(Some("java/lang/Number"), vec![]));
        members.insert(
            "com/acme/NumericBase".to_string(),
            cls(Some("java/lang/Object"), vec![method("getValue", "java/lang/Number", &[])]),
        );
        let mut producer = cls(Some("java/lang/Object"), vec![method("make", "java/lang/Object", &[])]);
        producer.flags.is_interface = true;
        members.insert("com/acme/Producer".to_string(), producer);
        let simple = [
            ("Object", "java/lang/Object"),
            ("Number", "java/lang/Number"),
            ("String", "java/lang/String"),
            ("Integer", "java/lang/Integer"),
            ("NumericBase", "com/acme/NumericBase"),
            ("Producer", "com/acme/Producer"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags(src: &str) -> Vec<String> {
        let symbols = bennu_java::prelude::extract_symbols(src);
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let nodes = crate::check::collect_nodes(tree.root_node());
        override_return_errors_in(&nodes, src, &symbols, &resolver())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    // ── positives ──────────────────────────────────────────────────────────────

    #[test]
    fn unrelated_covariant_return_is_flagged() {
        // `String getValue()` overriding `Number getValue()` — String is not a subtype of Number.
        let d = diags("class X extends NumericBase { public String getValue() { return \"\"; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("String") && d[0].contains("Number"), "{d:?}");
    }

    // ── negatives (must NEVER flag) ────────────────────────────────────────────

    #[test]
    fn legal_covariant_return_is_ok() {
        // `Integer getValue()` overriding `Number getValue()` — Integer IS-A Number → legal covariance.
        assert!(diags("class X extends NumericBase { public Integer getValue() { return 0; } }").is_empty());
    }

    #[test]
    fn identical_return_is_ok() {
        assert!(diags("class X extends NumericBase { public Number getValue() { return 0; } }").is_empty());
    }

    #[test]
    fn covariant_interface_return_is_ok() {
        // `String make()` implementing `Object make()` — String IS-A Object → legal.
        assert!(diags("class X implements Producer { public String make() { return \"\"; } }").is_empty());
    }

    #[test]
    fn different_params_is_an_overload_not_flagged() {
        // `String getValue(int i)` is a different signature than `Number getValue()` → overload, legal.
        let src = "class X extends NumericBase { public String getValue(int i) { return \"\"; } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn unresolved_supertype_is_not_flagged() {
        assert!(diags("class X extends Mystery { public String getValue() { return \"\"; } }").is_empty());
    }

    #[test]
    fn primitive_return_override_is_not_flagged() {
        // A `void` / primitive return isn't a reference type we reason about here → skipped (a genuine
        // primitive-vs-reference mismatch is a different, rarer error we deliberately don't chase).
        assert!(diags("class X extends NumericBase { public void getValue() {} }").is_empty());
    }

    #[test]
    fn static_method_is_not_an_override() {
        assert!(diags("class X extends NumericBase { public static String getValue() { return \"\"; } }").is_empty());
    }
}
