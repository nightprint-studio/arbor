//! Unknown-member diagnostics — a call `receiver.method(...)` whose `method` doesn't exist on the
//! receiver's inferred type. The first **resolver-backed** check: it runs `bennu-java`'s type
//! inference on the receiver, then looks the method up in that type's members (walking supertypes),
//! and flags only when the type is KNOWN and the method genuinely absent.
//!
//! Conservative — no false positives:
//!   * only `receiver.method(...)` (a call with an explicit receiver) is checked; a bare `foo()` or
//!     a static `Type.method()` (where inference yields no value type) is skipped;
//!   * we flag only when inference returns a type AND that type's members are known; if inference
//!     fails or the type (or any supertype in the walk) is unknown, we stay silent;
//!   * the member walk treats an unknown supertype as "might declare it" (returns `true`), so an
//!     un-indexed base class never causes a wrong "cannot resolve".

use bennu_java::prelude::{infer_receiver_type_at, FileSymbols, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::walk::hierarchy_has;

/// Parse `source` and flag calls to non-existent methods on their inferred receiver types.
pub fn unknown_members(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    unknown_members_in(tree.root_node(), source, &symbols, resolver)
}

/// The tree-driven core: reuses the caller's parsed `root` + extracted `symbols` (so validation
/// parses the file ONCE, not once per call site).
pub fn unknown_members_in(
    root: Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        if n.kind() == "method_invocation" {
            check_call(n, &root, source, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_call(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // Only `receiver.method(...)` — a bare `foo()` has no `object` field.
    if n.child_by_field_name("object").is_none() {
        return;
    }
    let Some(name) = n.child_by_field_name("name") else { return };
    if name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };

    // Infer the receiver's static type (at the method-name position, just past the `.`).
    let Some(ty) = infer_receiver_type_at(root, source, symbols, name.start_byte(), resolver) else {
        return;
    };
    if ty.binary_name.is_empty() {
        return;
    }
    // We can only assert a method is ABSENT when we actually know the receiver type's members.
    if resolver.members_of(&ty.binary_name).is_none() {
        return;
    }
    let has = hierarchy_has(resolver, &ty.binary_name, &|cm| {
        cm.methods.iter().any(|m| m.name == method && m.kind == MemberKind::Method)
    });
    if !has {
        out.push(Diagnostic {
            message: format!("Cannot resolve method `{method}` in `{}`", simple_name(&ty.binary_name)),
            severity: "error".to_string(),
            start: name.start_byte(),
            end: name.end_byte(),
        });
    }
}

pub(crate) fn simple_name(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef, Visibility};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// A fixed resolver: a `binary → members` map + a `simple → binary` table. The same shape the
    /// inherited/completion tests use.
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
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple(ret.to_string()),
            params: Vec::new(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            visibility: Visibility::Public,
            raw_signature: format!("{ret} {name}()"),
        }
    }

    /// String (super Object) with length/toUpperCase/trim; Object with toString/equals; List<E> with
    /// add/get/size. Enough to drive inference (declared locals, chains, generics, inheritance).
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method("toString", "java/lang/String"), method("equals", "boolean")],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "java/lang/String".to_string(),
            ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![
                    method("length", "int"),
                    method("toUpperCase", "java/lang/String"),
                    method("trim", "java/lang/String"),
                ],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "java/util/List".to_string(),
            ClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![
                    method("add", "boolean"),
                    method("get", "E"),
                    method("size", "int"),
                ],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        let simple = [
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
            ("List", "java/util/List"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("import java.util.List; class C {{ void m() {{ {body} }} }}");
        unknown_members(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn known_method_on_inferred_local_is_ok() {
        assert!(diags("String s = \"x\"; s.length();").is_empty());
    }

    #[test]
    fn unknown_method_on_inferred_local_is_flagged() {
        let d = diags("String s = \"x\"; s.lengthh();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("lengthh") && d[0].contains("String"), "{d:?}");
    }

    #[test]
    fn method_chain_return_type_is_inferred() {
        // trim() returns String → length() on the result is fine; a typo is caught.
        assert!(diags("String s = \"x\"; s.trim().length();").is_empty());
        assert_eq!(diags("String s = \"x\"; s.trim().lengthh();").len(), 1);
    }

    #[test]
    fn inherited_method_from_supertype_is_ok() {
        // toString() is declared on Object (String's superclass) — the walk must find it.
        assert!(diags("String s = \"x\"; s.toString();").is_empty());
    }

    #[test]
    fn generic_receiver_method_is_resolved() {
        // A List<String> local: add() exists, addd() doesn't. Exercises generic inference.
        assert!(diags("List<String> xs = null; xs.add(\"a\");").is_empty());
        let d = diags("List<String> xs = null; xs.addd(\"a\");");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("addd"));
    }

    #[test]
    fn generic_element_type_carries_through_the_chain() {
        // xs.get(0) → E → String; .length() on it is valid, a typo is caught.
        assert!(diags("List<String> xs = null; xs.get(0).length();").is_empty());
        assert_eq!(diags("List<String> xs = null; xs.get(0).lengthh();").len(), 1);
    }

    #[test]
    fn unknown_receiver_type_is_not_flagged() {
        // `mystery` has no declared type the resolver knows → inference yields nothing → silent.
        assert!(diags("Unknown mystery = null; mystery.whatever();").is_empty());
    }

    #[test]
    fn bare_and_static_calls_are_skipped() {
        // A bare call (no receiver) and a call whose receiver isn't a value type → not checked.
        assert!(diags("size();").is_empty());
    }
}
