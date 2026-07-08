//! Super-constructor diagnostics — when a class's superclass has **no no-arg constructor**, every
//! subclass constructor must explicitly chain (`super(args)` or `this(...)`), because the implicit
//! `super()` the compiler would insert doesn't exist. A subclass with *no* constructor at all is an
//! error too (its implicit default constructor would call the missing `super()`).
//!
//! Resolver-backed and conservative:
//!   * runs only when the superclass is known AND its constructors are indexed (it has at least one
//!     `<init>`). A project supertype whose constructors aren't in the index yields no `<init>`, so
//!     we can't tell it has no no-arg constructor → skip (a miss, never a false positive);
//!   * if the superclass has any zero-arg `<init>` (even a generated default), nothing is required.
//!
//! Today this therefore fires against **library / JDK** supertypes (constructors decoded from
//! bytecode). Project supertypes need their constructors indexed first.

use bennu_java::prelude::{extract_symbols, FileSymbols, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::resolve::type_binary;

/// Flag constructors that must call `super(...)` but don't (and classes lacking a needed constructor).
pub fn super_constructor_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    let nodes = crate::check::collect_nodes(tree.root_node());
    super_constructor_errors_in(&nodes, source, &symbols, resolver)
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn super_constructor_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "class_declaration" {
            check_class(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_class(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(sup_text) = superclass_text(n, bytes) else { return };
    let Some(binary) = type_binary(&sup_text, symbols, resolver) else { return };
    let Some(cm) = resolver.members_of(&binary) else { return };

    // Constructors of the superclass, by arity.
    let ctor_arities: Vec<usize> = cm
        .methods
        .iter()
        .filter(|m| m.name == "<init>" && m.kind == MemberKind::Method)
        .map(|m| m.params.len())
        .collect();
    if ctor_arities.is_empty() {
        return; // constructors not indexed → can't assert
    }
    if ctor_arities.iter().any(|&a| a == 0) {
        return; // a no-arg super constructor exists → implicit super() is fine
    }

    // The superclass needs an explicit super(args) call from every subclass constructor.
    let name = simple_name(&binary).to_string();
    let ctors = constructors(n);
    if ctors.is_empty() {
        // No constructor at all → the implicit default one would call the missing super().
        let anchor = n.child_by_field_name("name").unwrap_or(n);
        out.push(err(
            format!(
                "No default constructor available in `{name}` — this class needs a constructor that calls `super(...)`"
            ),
            anchor,
        ));
        return;
    }
    for ctor in ctors {
        if !chains_explicitly(ctor) {
            let anchor = ctor.child_by_field_name("name").unwrap_or(ctor);
            out.push(err(
                format!("Constructor must call `super(...)` — `{name}` has no no-arg constructor"),
                anchor,
            ));
        }
    }
}

/// The direct constructor declarations of a class (in its body).
fn constructors<'t>(class: Node<'t>) -> Vec<Node<'t>> {
    let Some(body) = class.child_by_field_name("body") else { return Vec::new() };
    let mut out = Vec::new();
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        if ch.kind() == "constructor_declaration" {
            out.push(ch);
        }
    }
    out
}

/// Whether a constructor's body begins with an explicit `super(...)` / `this(...)` chain call.
fn chains_explicitly(ctor: Node) -> bool {
    let Some(body) = ctor.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        if matches!(ch.kind(), "line_comment" | "block_comment") {
            continue;
        }
        // The chain call must be the FIRST statement.
        return ch.kind() == "explicit_constructor_invocation";
    }
    false
}

/// The written superclass type of a class (`extends X`), or `None`.
fn superclass_text(class: Node, bytes: &[u8]) -> Option<String> {
    let wrapper = class.child_by_field_name("superclass")?;
    let mut c = wrapper.walk();
    for ch in wrapper.named_children(&mut c) {
        if matches!(ch.kind(), "type_identifier" | "scoped_type_identifier" | "generic_type") {
            return ch.utf8_text(bytes).ok().map(str::to_string);
        }
    }
    None
}

fn err(message: String, node: Node) -> Diagnostic {
    crate::check_id::CheckId::SuperConstructorRequired.at(node, message)
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

    fn ctor(params: usize) -> Member {
        let params = (0..params).map(|_| TypeRef::simple("int")).collect();
        Member::method("<init>", TypeRef::simple("void"), params)
    }

    fn cls(ctors: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: ctors,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// `Base` with only a `Base(int)` ctor; `Zero` with a no-arg ctor; `Bare` with no ctors indexed.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("com/acme/Base".to_string(), cls(vec![ctor(1)]));
        members.insert("com/acme/Zero".to_string(), cls(vec![ctor(0), ctor(1)]));
        members.insert("com/acme/Bare".to_string(), cls(vec![]));
        let simple = [("Base", "com/acme/Base"), ("Zero", "com/acme/Zero"), ("Bare", "com/acme/Bare")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { members, simple }
    }

    fn diags(src: &str) -> Vec<String> {
        super_constructor_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn missing_super_call_is_flagged() {
        let d = diags("class X extends Base { X() { int a = 1; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("super") && d[0].contains("Base"), "{d:?}");
    }

    #[test]
    fn explicit_super_is_ok() {
        assert!(diags("class X extends Base { X() { super(1); } }").is_empty());
    }

    #[test]
    fn this_chain_is_ok() {
        assert!(diags("class X extends Base { X() { this(1); } X(int a) { super(a); } }").is_empty());
    }

    #[test]
    fn no_constructor_at_all_is_flagged() {
        let d = diags("class X extends Base {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("No default constructor"), "{d:?}");
    }

    #[test]
    fn superclass_with_no_arg_ctor_is_ok() {
        assert!(diags("class X extends Zero { X() {} }").is_empty());
        assert!(diags("class X extends Zero {}").is_empty());
    }

    #[test]
    fn superclass_without_indexed_ctors_is_skipped() {
        // `Bare` has no <init> in the index → we can't assert → silent.
        assert!(diags("class X extends Bare { X() {} }").is_empty());
    }

    #[test]
    fn unknown_superclass_is_skipped() {
        assert!(diags("class X extends Mystery { X() {} }").is_empty());
    }
}
