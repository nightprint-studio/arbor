//! **Impossible-equals** diagnostic — a call `receiver.equals(arg)` where the receiver type and the
//! argument type are provably disjoint, so `.equals()` can never return `true` at runtime.
//!
//! The check is conservative: it fires only when ALL of the following hold:
//!   * the call is `<expr>.equals(<single-arg>)` with an explicit receiver;
//!   * both sides infer to a resolvable, non-Object, non-type-variable reference type;
//!   * neither side is an array (array subtyping is complex);
//!   * neither side is an interface (any class could implement it at runtime);
//!   * at least one side is concrete-final (`is_final || is_enum`) — without that, two non-final
//!     classes still allow subclass overlap and the flag would be a false positive;
//!   * `subtype_verdict` returns `Some(false)` in BOTH directions — neither extends the other.

use bennu_java::prelude::{infer_node_type_cached, subtype_verdict, FileSymbols, InferCache, TypeRef, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::{is_type_var, simple_name};

const OBJECT: &str = "java/lang/Object";

/// Parse `source` and flag impossible `equals()` calls.
pub fn impossible_equals_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::engine::check::collect_nodes(root);
    impossible_equals_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` and reuses the inference cache.
pub fn impossible_equals_in(
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
        if n.kind() != "method_invocation" {
            continue;
        }
        if let Some(d) = check_equals_call(n, &root, source, bytes, symbols, resolver, cache) {
            out.push(d);
        }
    }
    out
}

/// The wrapper class a primitive is boxed to; any other name comes back unchanged.
fn boxed(binary: &str) -> &str {
    match binary {
        "int" => "java/lang/Integer",
        "long" => "java/lang/Long",
        "short" => "java/lang/Short",
        "byte" => "java/lang/Byte",
        "char" => "java/lang/Character",
        "boolean" => "java/lang/Boolean",
        "float" => "java/lang/Float",
        "double" => "java/lang/Double",
        other => other,
    }
}

fn check_equals_call(
    call: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<Diagnostic> {
    // Must be `<receiver>.equals(...)` — an explicit receiver is required.
    let receiver_node = call.child_by_field_name("object")?;
    let name_node = call.child_by_field_name("name")?;
    if name_node.utf8_text(bytes).ok()? != "equals" {
        return None;
    }

    // Exactly one argument.
    let args = call.child_by_field_name("arguments")?;
    let arg_nodes: Vec<Node> = {
        let mut c = args.walk();
        args.named_children(&mut c).collect()
    };
    if arg_nodes.len() != 1 {
        return None;
    }
    let arg_node = arg_nodes[0];

    // Infer receiver and argument types.
    let recv_type = infer_node_type_cached(root, source, symbols, &receiver_node, resolver, cache)?;
    let arg_type = infer_node_type_cached(root, source, symbols, &arg_node, resolver, cache)?;

    let recv_bin = recv_type.binary_name.as_str();
    // `equals(Object)` boxes a primitive argument: `longBox.equals(1)` compares a `Long` with an `Integer`.
    let arg_bin = if arg_type.dims == 0 { boxed(arg_type.binary_name.as_str()) } else { arg_type.binary_name.as_str() };

    // Skip empty, Object (erased type var), type variables, arrays.
    if recv_bin.is_empty() || arg_bin.is_empty() {
        return None;
    }
    if recv_bin == OBJECT || arg_bin == OBJECT {
        return None;
    }
    if is_type_var(recv_bin) || is_type_var(arg_bin) {
        return None;
    }
    if recv_type.dims > 0 || arg_type.dims > 0 {
        return None;
    }

    // Both must be resolvable for a sound verdict.
    let recv_members = resolver.members_of(recv_bin)?;
    let arg_members = resolver.members_of(arg_bin)?;

    let recv_flags = recv_members.flags;
    let arg_flags = arg_members.flags;

    // Skip if either side is an interface.
    if recv_flags.is_interface || arg_flags.is_interface {
        return None;
    }

    // At least one side must be concrete-final.
    let recv_final = recv_flags.is_final || recv_flags.is_enum;
    let arg_final = arg_flags.is_final || arg_flags.is_enum;
    if !recv_final && !arg_final {
        return None;
    }

    // Neither must be a subtype of the other.
    if subtype_verdict(resolver, &TypeRef::simple(recv_bin), &TypeRef::simple(arg_bin)) != Some(false) {
        return None;
    }
    if subtype_verdict(resolver, &TypeRef::simple(arg_bin), &TypeRef::simple(recv_bin)) != Some(false) {
        return None;
    }

    let recv_simple = simple_name(recv_bin);
    let arg_simple = simple_name(arg_bin);
    Some(CheckId::ImpossibleEquals.at(
        call,
        format!("`{recv_simple}` and `{arg_simple}` are unrelated types — `.equals()` can never return `true`"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import};
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
        fn resolve_simple_name(&self, name: &str, _: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn final_class(superclass: &str) -> ClassMembers {
        ClassMembers {
            superclass: Some(TypeRef::simple(superclass)),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags { is_final: true, ..Default::default() },
            type_params: Vec::new(),
        }
    }

    fn class(superclass: &str) -> ClassMembers {
        ClassMembers {
            superclass: Some(TypeRef::simple(superclass)),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
            type_params: Vec::new(),
        }
    }

    fn interface_type() -> ClassMembers {
        ClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags { is_interface: true, is_abstract: true, ..Default::default() },
            type_params: Vec::new(),
        }
    }

    /// Minimal JDK mock with Boolean, String, Integer (all final), Number, Serializable.
    fn resolver() -> MapResolver {
        let mut members: HashMap<String, ClassMembers> = HashMap::new();
        let mut simple: HashMap<String, String> = HashMap::new();

        // java.lang.Object — root
        members.insert("java/lang/Object".to_string(), ClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
            type_params: Vec::new(),
        });
        // java.lang.Number — abstract
        members.insert("java/lang/Number".to_string(), ClassMembers {
            superclass: Some(TypeRef::simple("java/lang/Object")),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags { is_abstract: true, ..Default::default() },
            type_params: Vec::new(),
        });
        // java.io.Serializable — interface
        members.insert("java/io/Serializable".to_string(), interface_type());
        // java.lang.Boolean — final, extends Object
        members.insert("java/lang/Boolean".to_string(), final_class("java/lang/Object"));
        // java.lang.String — final, extends Object
        members.insert("java/lang/String".to_string(), final_class("java/lang/Object"));
        // java.lang.Integer — final, extends Number
        members.insert("java/lang/Integer".to_string(), final_class("java/lang/Number"));
        // java.lang.Long — final, extends Number
        members.insert("java/lang/Long".to_string(), final_class("java/lang/Number"));
        // com.example.Foo — non-final, extends Object
        members.insert("com/example/Foo".to_string(), class("java/lang/Object"));
        // com.example.Bar — non-final, extends Object
        members.insert("com/example/Bar".to_string(), class("java/lang/Object"));

        simple.insert("Boolean".to_string(), "java/lang/Boolean".to_string());
        simple.insert("String".to_string(), "java/lang/String".to_string());
        simple.insert("Integer".to_string(), "java/lang/Integer".to_string());
        simple.insert("Long".to_string(), "java/lang/Long".to_string());
        simple.insert("Number".to_string(), "java/lang/Number".to_string());
        simple.insert("Foo".to_string(), "com/example/Foo".to_string());
        simple.insert("Bar".to_string(), "com/example/Bar".to_string());

        MapResolver { members, simple }
    }

    fn messages(body: &str) -> Vec<String> {
        let r = resolver();
        let src = format!("import java.lang.*; class C {{ void m(String s, Boolean b, Integer i, Long l, int p, long q, Number n, com.example.Foo foo, com.example.Bar bar) {{ {body} }} }}");
        impossible_equals_errors(&src, &r).into_iter().map(|d| d.message).collect()
    }

    fn codes(body: &str) -> Vec<String> {
        let r = resolver();
        let src = format!("import java.lang.*; class C {{ void m(String s, Boolean b, Integer i, Long l, int p, long q, Number n, com.example.Foo foo, com.example.Bar bar) {{ {body} }} }}");
        impossible_equals_errors(&src, &r).into_iter().map(|d| d.code).collect()
    }

    // ── should flag ──────────────────────────────────────────────────────────────

    #[test]
    fn boolean_equals_string_flagged() {
        let m = messages("b.equals(s);");
        assert!(m.iter().any(|msg| msg.contains("Boolean") && msg.contains("String")), "{m:?}");
    }

    #[test]
    fn integer_equals_string_flagged() {
        let m = messages("i.equals(s);");
        assert!(m.iter().any(|msg| msg.contains("Integer") && msg.contains("String")), "{m:?}");
    }

    #[test]
    fn string_equals_boolean_flagged() {
        let m = messages("s.equals(b);");
        assert!(m.iter().any(|msg| msg.contains("String") && msg.contains("Boolean")), "{m:?}");
    }

    #[test]
    fn code_is_impossible_equals() {
        let c = codes("b.equals(s);");
        assert!(c.iter().any(|code| code == "impossible-equals"), "{c:?}");
    }

    #[test]
    fn long_equals_int_primitive_is_flagged() {
        let m = messages("l.equals(p);");
        assert!(m.iter().any(|msg| msg.contains("Long") && msg.contains("Integer")), "{m:?}");
    }

    #[test]
    fn integer_equals_long_primitive_is_flagged() {
        let m = messages("i.equals(q);");
        assert!(m.iter().any(|msg| msg.contains("Integer") && msg.contains("Long")), "{m:?}");
    }

    // ── should NOT flag ──────────────────────────────────────────────────────────

    #[test]
    fn integer_equals_int_primitive_not_flagged() {
        assert!(messages("i.equals(p);").is_empty());
    }

    #[test]
    fn number_equals_int_primitive_not_flagged() {
        assert!(messages("n.equals(p);").is_empty());
    }

    #[test]
    fn same_type_not_flagged() {
        let m = messages("b.equals(b);");
        assert!(m.is_empty(), "{m:?}");
    }

    #[test]
    fn string_equals_string_not_flagged() {
        let m = messages("s.equals(s);");
        assert!(m.is_empty(), "{m:?}");
    }

    #[test]
    fn both_non_final_not_flagged() {
        // Foo and Bar are both non-final — subclass could bridge them.
        let m = messages("foo.equals(bar);");
        assert!(m.is_empty(), "{m:?}");
    }
}
