//! A simple name that two on-demand imports both supply — javac's `ref.ambiguous`.
//!
//! `import java.awt.*; import java.util.*;` compiles until the file writes `List`, and then it does
//! not: both packages have one, and neither import outranks the other. The same holds for the static
//! form — `import static java.lang.Integer.*; import static java.lang.Long.*;` and a bare `MAX_VALUE`.
//!
//! An on-demand import shadows nothing and is shadowed by almost everything (JLS §6.4.1, §7.5), so a
//! name is ambiguous only when nothing closer binds it:
//!
//!   * **a type** — not declared in this file (nested types included), not a type parameter, not
//!     named by a single-type or single-static import, not a type of the file's own package, not a
//!     member type inherited by a type of this file;
//!   * **a field** — not a local or parameter in scope, not a field of the top-level type or anything
//!     it inherits, not named by a single-static import;
//!   * **a method** — not a method of the top-level type or anything it inherits, not named by a
//!     single-static import; and then only when the overload the call binds to is declared, with the
//!     same signature, by two of the imported classes. `toHexString(1)` against `Integer.*` and
//!     `Long.*` picks `Integer`'s `int` overload and is fine.
//!
//! Two DIFFERENT declarations must be found: a class imported twice, or a member reached through a
//! subclass and its superclass, is one declaration and no ambiguity. Judged only on a complete
//! classpath — without one, a type of the file's own package in a jar nobody indexed could be the
//! declaration that shadows both imports.

use std::collections::HashSet;

use bennu_java::prelude::{
    inherited_member_type_of, overload_fit, same_binary_type, static_import_targets, walk, FileSymbols,
    InferCache, Member, MemberKind, OverloadFit, TypeRef, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::simple_name;

/// Every use of a name two on-demand imports make ambiguous.
pub fn import_ambiguity_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    classpath_complete: bool,
) -> Vec<Diagnostic> {
    if !classpath_complete || !symbols.imports.iter().any(|i| i.star) {
        return Vec::new();
    }
    let file = FileImports::new(root, source, symbols, resolver);
    let mut out = Vec::new();
    for &n in nodes {
        let found = match n.kind() {
            "type_identifier" => file.ambiguous_type(n),
            "identifier" => file.ambiguous_field(n),
            "method_invocation" => file.ambiguous_call(n, cache),
            _ => None,
        };
        if let Some((anchor, message)) = found {
            out.push(CheckId::AmbiguousReference.at(anchor, message));
        }
    }
    out
}

/// What the file's imports and declarations bind, gathered once.
struct FileImports<'a, 't> {
    root: Node<'t>,
    source: &'a str,
    bytes: &'a [u8],
    symbols: &'a FileSymbols,
    resolver: &'a dyn TypeResolver,
    /// `import a.b.*;` — packages, or types whose member types come in.
    type_on_demand: Vec<String>,
    /// `import static a.B.*;` — the owner types.
    static_on_demand: Vec<String>,
    /// Simple names a single-type or single-static import binds.
    single_names: HashSet<String>,
    /// Types declared in this file and type parameters, by simple name.
    declared_names: HashSet<String>,
    /// The supertypes of this file's types, or `None` when one of them does not resolve — an
    /// unreadable supertype may declare the member type that would shadow an import.
    supertype_roots: Option<Vec<String>>,
    /// The single top-level type, when its own members can be read to the end.
    top: Option<(Node<'t>, String)>,
}

impl<'a, 't> FileImports<'a, 't> {
    fn new(root: Node<'t>, source: &'a str, symbols: &'a FileSymbols, resolver: &'a dyn TypeResolver) -> Self {
        let bytes = source.as_bytes();
        let mut type_on_demand = Vec::new();
        let mut single_names = HashSet::new();
        for import in &symbols.imports {
            match (import.static_, import.star) {
                (false, true) => type_on_demand.push(import.path.replace('.', "/")),
                (_, false) => {
                    single_names.insert(import.path.rsplit('.').next().unwrap_or(&import.path).to_string());
                }
                (true, true) => {}
            }
        }
        let static_on_demand = static_import_targets(&symbols.imports)
            .into_iter()
            .filter(|t| t.member.is_none())
            .map(|t| t.owner_binary)
            .collect();
        let mut declared_names: HashSet<String> = symbols.types.iter().map(|t| t.name.clone()).collect();
        collect_type_parameters(root, bytes, &mut declared_names);
        let top = crate::support::scopes::single_top_level_type(root, bytes)
            .filter(|t| !crate::support::nodes::generated_names(t.node, bytes).values)
            .and_then(|t| {
                let binary = crate::support::resolve::type_binary(&t.decl_name, symbols, resolver)?;
                crate::support::walk::hierarchy_fully_known(resolver, &binary).then_some((t.node, binary))
            });
        FileImports {
            root,
            source,
            bytes,
            symbols,
            resolver,
            type_on_demand,
            static_on_demand,
            single_names,
            declared_names,
            supertype_roots: supertype_roots(symbols, resolver),
            top,
        }
    }

    // ── types ────────────────────────────────────────────────────────────────

    fn ambiguous_type(&self, n: Node<'t>) -> Option<(Node<'t>, String)> {
        let parent = n.parent()?;
        if matches!(parent.kind(), "scoped_type_identifier" | "type_parameter" | "method_reference") {
            return None;
        }
        let name = n.utf8_text(self.bytes).ok()?;
        if self.declared_names.contains(name) || self.single_names.contains(name) {
            return None;
        }
        if let Some(own) = crate::support::resolve::same_package_binary(name, self.symbols) {
            if self.resolver.members_of(&own).is_some() {
                return None;
            }
        }
        let roots = self.supertype_roots.as_ref()?;
        if roots.iter().any(|r| inherited_member_type_of(self.resolver, r, name).is_some()) {
            return None;
        }
        let mut found: Vec<String> = Vec::new();
        let mut add = |binary: String| {
            if !found.iter().any(|f| same_binary_type(f, &binary)) {
                found.push(binary);
            }
        };
        for imported in &self.type_on_demand {
            for candidate in [format!("{imported}/{name}"), format!("{imported}${name}")] {
                if self.resolver.members_of(&candidate).is_some() {
                    add(candidate);
                    break;
                }
            }
        }
        for owner in &self.static_on_demand {
            if let Some(member) = inherited_member_type_of(self.resolver, owner, name) {
                add(member);
            }
        }
        let [first, second, ..] = found.as_slice() else { return None };
        Some((
            n,
            format!(
                "`{name}` is ambiguous: both `{}` and `{}` are imported on demand",
                dotted(first),
                dotted(second)
            ),
        ))
    }

    // ── fields ───────────────────────────────────────────────────────────────

    fn ambiguous_field(&self, n: Node<'t>) -> Option<(Node<'t>, String)> {
        if self.static_on_demand.len() < 2 || !crate::support::scopes::is_value_position(n) {
            return None;
        }
        let (top, top_binary) = self.top.as_ref()?;
        if !crate::support::scopes::scope_is_top_across_lambdas(n, *top) {
            return None;
        }
        let name = n.utf8_text(self.bytes).ok()?;
        if self.single_names.contains(name) || crate::support::scopes::resolves_as_local_lexically(n, *top, self.bytes) {
            return None;
        }
        let own_field = walk(self.resolver, &TypeRef::simple(top_binary.clone()), |a| {
            a.members.fields.iter().any(|f| f.name == name && f.kind == MemberKind::Field).then_some(())
        });
        if own_field.found.is_some() || !own_field.complete || declares_enum_constant(*top, name, self.bytes) {
            return None;
        }
        let mut declaring: Vec<String> = Vec::new();
        for owner in &self.static_on_demand {
            let hit = walk(self.resolver, &TypeRef::simple(owner.clone()), |a| {
                a.members
                    .fields
                    .iter()
                    .any(|f| f.name == name && f.is_static && f.kind == MemberKind::Field)
                    .then(|| a.ty.binary_name.clone())
            });
            if let Some(d) = hit.found {
                if !declaring.iter().any(|x| same_binary_type(x, &d)) {
                    declaring.push(d);
                }
            }
        }
        let [first, second, ..] = declaring.as_slice() else { return None };
        Some((
            n,
            format!(
                "`{name}` is ambiguous: both `{}.{name}` and `{}.{name}` are imported on demand",
                simple_name(first),
                simple_name(second)
            ),
        ))
    }

    // ── methods ──────────────────────────────────────────────────────────────

    fn ambiguous_call(&self, call: Node<'t>, cache: &InferCache) -> Option<(Node<'t>, String)> {
        if self.static_on_demand.len() < 2 || call.child_by_field_name("object").is_some() {
            return None;
        }
        let (top, top_binary) = self.top.as_ref()?;
        if !crate::support::scopes::scope_is_top_across_lambdas(call, *top) {
            return None;
        }
        let name_node = call.child_by_field_name("name")?;
        let name = name_node.utf8_text(self.bytes).ok()?;
        if self.single_names.contains(name) {
            return None;
        }
        let own = cache.resolve_methods(self.resolver, top_binary, name);
        if !own.complete || !own.candidates.is_empty() || declares_method(*top, name, self.bytes) {
            return None;
        }
        // Every static method of that name the imports bring in, with the class that declares it.
        let mut members: Vec<Member> = Vec::new();
        let mut owners: Vec<String> = Vec::new();
        for owner in &self.static_on_demand {
            let mut seen_complete = true;
            let walked = walk::<()>(self.resolver, &TypeRef::simple(owner.clone()), |a| {
                for m in a.members.methods.iter().filter(|m| m.name == name && m.is_static) {
                    let known = members.iter().zip(&owners).any(|(k, o)| k.params == m.params && same_binary_type(o, &a.ty.binary_name));
                    if !known {
                        members.push(m.clone());
                        owners.push(a.ty.binary_name.clone());
                    }
                }
                None
            });
            seen_complete &= walked.complete;
            if !seen_complete {
                return None;
            }
        }
        let fit = overload_fit(&self.root, self.source, self.symbols, &call, &members, self.resolver, cache);
        let OverloadFit::Applicable(kept) = fit else { return None };
        let [bound] = kept.as_slice() else { return None };
        let mut declaring: Vec<&String> = Vec::new();
        for (m, owner) in members.iter().zip(&owners) {
            if m.params == bound.params && !declaring.iter().any(|d| same_binary_type(d, owner)) {
                declaring.push(owner);
            }
        }
        let [first, second, ..] = declaring.as_slice() else { return None };
        Some((
            name_node,
            format!(
                "Ambiguous call to `{name}`: `{}.{name}` and `{}.{name}` both match, and both are imported on demand",
                simple_name(first),
                simple_name(second)
            ),
        ))
    }
}

/// `a/b/C$D` → `a.b.C.D`.
fn dotted(binary: &str) -> String {
    binary.replace(['/', '$'], ".")
}

/// Every type-parameter name declared anywhere in the file.
fn collect_type_parameters(root: Node, bytes: &[u8], out: &mut HashSet<String>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "type_parameter" {
            let mut c = n.walk();
            let name = n.named_children(&mut c).find(|x| matches!(x.kind(), "type_identifier" | "identifier"));
            if let Some(text) = name.and_then(|x| x.utf8_text(bytes).ok()) {
                out.insert(text.to_string());
            }
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
}

/// The supertypes written by the file's types, or `None` when one does not resolve.
fn supertype_roots(symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Option<Vec<String>> {
    let mut roots = Vec::new();
    for declared in &symbols.types {
        for written in declared.extends.iter().chain(&declared.implements) {
            let binary = crate::support::resolve::type_binary(written, symbols, resolver)?;
            if !crate::support::walk::hierarchy_fully_known(resolver, &binary) {
                return None;
            }
            roots.push(binary);
        }
    }
    Some(roots)
}

/// Whether the top-level type is an enum declaring the constant `name`.
fn declares_enum_constant(top: Node, name: &str, bytes: &[u8]) -> bool {
    if top.kind() != "enum_declaration" {
        return false;
    }
    let Some(body) = top.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    let found = body.named_children(&mut c).any(|m| {
        m.kind() == "enum_constant" && m.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name)
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// `lib/A` and `other/B` each declare a static `VALUE`, a static `shared(int)`, a nested `Widget`;
    /// `lib/Only` has `solo(int)`; `awt/List` and `util/List` share a simple name; `util/Map` does not.
    struct MapResolver(HashMap<String, ClassMembers>);

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            (name == "C").then(|| "p/C".to_string())
        }
    }

    fn class(methods: Vec<Member>, fields: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: None,
            interfaces: Vec::new(),
            methods,
            fields,
            flags: Default::default(),
        }
    }

    fn resolver() -> MapResolver {
        let owner = || {
            class(
                vec![Member::method("shared", TypeRef::simple("int"), vec![TypeRef::simple("int")]).stat()],
                vec![Member::field("VALUE", TypeRef::simple("int")).stat().final_()],
            )
        };
        let mut m = HashMap::new();
        m.insert("lib/A".to_string(), owner());
        m.insert("other/B".to_string(), owner());
        m.insert("lib/A/Widget".to_string(), class(vec![], vec![]));
        m.insert("other/B/Widget".to_string(), class(vec![], vec![]));
        m.insert(
            "lib/Only".to_string(),
            class(vec![Member::method("solo", TypeRef::simple("int"), vec![TypeRef::simple("int")]).stat()], vec![]),
        );
        m.insert("awt/List".to_string(), class(vec![], vec![]));
        m.insert("util/List".to_string(), class(vec![], vec![]));
        m.insert("util/Map".to_string(), class(vec![], vec![]));
        m.insert("p/C".to_string(), class(vec![], vec![]));
        MapResolver(m)
    }

    fn diags(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let symbols = bennu_java::prelude::extract_symbols(src);
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        import_ambiguity_errors_in(tree.root_node(), &nodes, src, &symbols, &resolver(), &InferCache::new(), true)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    const STATIC: &str = "package p;\nimport static lib.A.*;\nimport static other.B.*;\nimport static lib.Only.*;\n";

    #[test]
    fn a_type_two_package_wildcards_supply_is_ambiguous() {
        let d = diags("package p;\nimport awt.*;\nimport util.*;\nclass C { List a; Map b; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("awt.List") && d[0].contains("util.List"), "{d:?}");
    }

    #[test]
    fn a_single_type_import_or_a_declared_type_settles_it() {
        assert!(diags("package p;\nimport awt.*;\nimport util.*;\nimport util.List;\nclass C { List a; }").is_empty());
        assert!(diags("package p;\nimport awt.*;\nimport util.*;\nclass C { static class List {} List a; }").is_empty());
    }

    #[test]
    fn a_static_field_two_wildcards_supply_is_ambiguous() {
        let d = diags(&format!("{STATIC}class C {{ int m() {{ return VALUE; }} }}"));
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn a_local_or_a_single_static_import_settles_a_field() {
        assert!(diags(&format!("{STATIC}class C {{ int m() {{ int VALUE = 1; return VALUE; }} }}")).is_empty());
        let single = "package p;\nimport static lib.A.*;\nimport static other.B.*;\nimport static lib.A.VALUE;\nclass C { int m() { return VALUE; } }";
        assert!(diags(single).is_empty());
    }

    #[test]
    fn a_call_both_classes_answer_with_one_signature_is_ambiguous() {
        let d = diags(&format!("{STATIC}class C {{ void m() {{ shared(1); }} }}"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(diags(&format!("{STATIC}class C {{ void m() {{ solo(1); }} }}")).is_empty());
    }

    #[test]
    fn an_own_method_shadows_every_imported_one() {
        assert!(diags(&format!("{STATIC}class C {{ static int shared(int v) {{ return v; }} void m() {{ shared(1); }} }}")).is_empty());
    }

    #[test]
    fn a_nested_type_two_static_wildcards_supply_is_ambiguous() {
        let d = diags(&format!("{STATIC}class C {{ Widget w; }}"));
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn nothing_is_judged_without_a_complete_classpath() {
        let src = "package p;\nimport awt.*;\nimport util.*;\nclass C { List a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let symbols = bennu_java::prelude::extract_symbols(src);
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        let got = import_ambiguity_errors_in(tree.root_node(), &nodes, src, &symbols, &resolver(), &InferCache::new(), false);
        assert!(got.is_empty());
    }
}

/// Whether any type written in the top-level declaration declares a method `name` — the buffer's
/// own answer, ahead of an index that may not have seen it yet.
fn declares_method(top: Node, name: &str, bytes: &[u8]) -> bool {
    let mut stack = vec![top];
    while let Some(n) = stack.pop() {
        if n.kind() == "method_declaration"
            && n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(name)
        {
            return true;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    false
}
