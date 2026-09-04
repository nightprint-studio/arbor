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

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::nodes::{simple_name};

/// Parse `source` and flag calls to non-existent methods on their inferred receiver types.
pub fn unknown_members(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    unknown_members_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// The tree-driven core: iterates the caller's pre-collected `nodes` (one shared DFS) and reuses
/// `root` (for inference) + `symbols` + the shared per-file inference `cache`.
pub fn unknown_members_in(
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
                check_call(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            // `Type::method` / `obj::method` — the same question, and one nothing asked. It is also
            // the construct a half-applied rename breaks most often, because the reference index
            // did not see method references at all until recently: the declaration moved and every
            // `::` site was left naming something gone, silently.
            "method_reference" => {
                check_reference(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            _ => {}
        }
    }
    out
}

/// `Qualifier::member` — flag when the qualifier's type is KNOWN and declares no such method.
///
/// As conservative as [`check_call`]: an unresolved qualifier, an unknown type, or an unknown
/// supertype in the walk all stay silent. `Foo::new` names a constructor, not a method, and is
/// skipped.
#[allow(clippy::too_many_arguments)]
fn check_reference(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let mut c = n.walk();
    let children: Vec<Node> = n.named_children(&mut c).collect();
    let (Some(qualifier), Some(name)) = (children.first(), children.last()) else { return };
    // One named child means `Foo::new`: the "name" would be the qualifier itself.
    if qualifier.id() == name.id() || name.kind() != "identifier" || name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };
    // The qualifier as a VALUE (`obj::run`), else as a TYPE (`Util::create`).
    let inferred = infer_node_type_cached(root, source, symbols, qualifier, resolver, cache)
        .or_else(|| {
            let text = qualifier.utf8_text(bytes).ok()?;
            let scope = crate::resolve::enclosing_scope(*qualifier, bytes, symbols);
            let binary = bennu_java::prelude::resolve_written_type(
                text,
                &crate::type_scope::FileScope { symbols, resolver, scope },
            )
            .resolved()?;
            resolver
                .members_of(&binary)
                .is_some()
                .then(|| bennu_java::prelude::TypeRef::simple(binary))
        });
    let Some(ty) = inferred else { return };
    if resolver.members_of(&ty.binary_name).is_none() {
        return;
    }
    let has = crate::walk::hierarchy_has(resolver, &ty.binary_name, &|cm| {
        cm.methods.iter().any(|m| m.name == method)
    });
    if !has {
        out.push(crate::check_id::CheckId::UnknownMember.at(
            *name,
            format!(
                "Cannot resolve method `{method}` in `{}`",
                ty.binary_name.rsplit('/').next().unwrap_or(&ty.binary_name)
            ),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn check_call(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    // Only `receiver.method(...)` — a bare `foo()` has no `object` field.
    let Some(obj) = n.child_by_field_name("object") else { return };
    let Some(name) = n.child_by_field_name("name") else { return };
    if name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };

    // Infer the receiver's static type from the already-located `object` node (no descendant search).
    let Some(ty) = infer_node_type_cached(root, source, symbols, &obj, resolver, cache) else {
        return;
    };
    if ty.binary_name.is_empty() {
        return;
    }
    // Shared, memoized hierarchy walk (see `InferCache::resolve_methods`): one traversal per
    // `(receiver type, method)` feeds this check + arity + argument-type, across every call site.
    let res = cache.resolve_methods(resolver, &ty.binary_name, method);
    // Conservative: a match anywhere in the hierarchy, OR an incomplete hierarchy (an unknown
    // supertype — including the receiver type itself being unknown — might declare the method).
    let has = !res.candidates.is_empty() || !res.complete;
    if !has {
        out.push(crate::check_id::CheckId::UnknownMember.at(
            name,
            format!("Cannot resolve method `{method}` in `{}`", simple_name(&ty.binary_name)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
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
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new()).sig(format!("{ret} {name}()"))
    }

    /// String (super Object) with length/toUpperCase/trim; Object with toString/equals; List<E> with
    /// add/get/size. Enough to drive inference (declared locals, chains, generics, inheritance).
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                type_params: Vec::new(),
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
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
                type_params: Vec::new(),
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

    #[test]
    fn method_type_variable_from_a_chain_is_not_misresolved() {
        // Regression: `xs.stream().map(f).collect(toList()).indexOf(u)` must NOT flag `indexOf` as
        // missing. `Stream.collect` returns a METHOD-level type variable (`R`), NOT the stream's
        // element type — the old heuristic mapped `R` to the receiver's sole type argument, so
        // `collect(...)` inferred as `Attachment` and `.indexOf(…)` resolved against `Attachment`
        // (which lacks it) → a false "cannot resolve method". `R` now stays unresolved → the call is
        // skipped conservatively.
        fn gen(bn: &str, args: Vec<TypeRef>) -> TypeRef {
            TypeRef { binary_name: bn.to_string(), dims: 0, type_args: args }
        }
        fn ty(type_params: Vec<&str>, superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
            ClassMembers {
                type_params: type_params.into_iter().map(str::to_string).collect(),
                superclass: superclass.map(TypeRef::simple),
                interfaces: Vec::new(),
                methods,
                fields: Vec::new(),
                flags: Default::default(),
            }
        }
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), ty(vec![], None, vec![]));
        members.insert("com/acme/Attachment".to_string(), ty(vec![], Some("java/lang/Object"), vec![]));
        members.insert(
            "java/util/List".to_string(),
            ty(
                vec!["E"],
                Some("java/lang/Object"),
                vec![
                    Member::method("stream", gen("java/util/stream/Stream", vec![TypeRef::simple("E")]), vec![]),
                    Member::method("indexOf", TypeRef::simple("int"), vec![TypeRef::simple("java/lang/Object")]),
                ],
            ),
        );
        members.insert(
            "java/util/stream/Stream".to_string(),
            ty(
                vec!["T"],
                Some("java/lang/Object"),
                vec![
                    // `<R> Stream<R> map(Function<? super T, ? extends R>)` — return carries the METHOD var R.
                    Member::method("map", gen("java/util/stream/Stream", vec![TypeRef::simple("R")]), vec![TypeRef::simple("java/lang/Object")]),
                    // `<R,A> R collect(Collector<? super T,A,R>)` — return IS the method var R.
                    Member::method("collect", TypeRef::simple("R"), vec![TypeRef::simple("java/lang/Object")]),
                ],
            ),
        );
        let simple = [
            ("List", "java/util/List"),
            ("Attachment", "com/acme/Attachment"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        let resolver = MapResolver { members, simple };
        let src = "import java.util.List; class C { void m() { \
                   List<Attachment> xs = null; xs.stream().map(xs).collect(xs).indexOf(xs); } }";
        let d: Vec<String> =
            unknown_members(src, &resolver).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "expected no cannot-resolve on the stream chain, got {d:?}");
    }

    #[test]
    fn untyped_lambda_param_is_target_typed_not_a_shadowed_field() {
        // `svc.consume(result -> result.getName())`: `result` is an UNTYPED lambda parameter bound to
        // `Consumer<Attachment>` (consume's parameter), so it types as `Attachment` — NOT the enclosing
        // class's same-named `String result` field. Before the fix the field won and `result.getName()`
        // was falsely flagged (String has no getName).
        fn gen(bn: &str, args: Vec<TypeRef>) -> TypeRef {
            TypeRef { binary_name: bn.to_string(), dims: 0, type_args: args }
        }
        fn ty(type_params: Vec<&str>, superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
            ClassMembers {
                type_params: type_params.into_iter().map(str::to_string).collect(),
                superclass: superclass.map(TypeRef::simple),
                interfaces: Vec::new(),
                methods,
                fields: Vec::new(),
                flags: Default::default(),
            }
        }
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), ty(vec![], None, vec![]));
        members.insert(
            "java/lang/String".to_string(),
            ty(vec![], Some("java/lang/Object"), vec![Member::method("length", TypeRef::simple("int"), vec![])]),
        );
        members.insert(
            "com/acme/Attachment".to_string(),
            ty(vec![], Some("java/lang/Object"), vec![Member::method("getName", TypeRef::simple("java/lang/String"), vec![])]),
        );
        // `Consumer<T>` with its single abstract method `accept(T)` — the SAM the param is read from.
        members.insert(
            "java/util/function/Consumer".to_string(),
            ty(vec!["T"], None, vec![Member::method("accept", TypeRef::simple("void"), vec![TypeRef::simple("T")]).abstract_()]),
        );
        members.insert(
            "com/acme/Svc".to_string(),
            ty(
                vec![],
                Some("java/lang/Object"),
                vec![Member::method("consume", TypeRef::simple("void"), vec![gen("java/util/function/Consumer", vec![TypeRef::simple("com/acme/Attachment")])])],
            ),
        );
        let simple = [
            ("Svc", "com/acme/Svc"),
            ("Attachment", "com/acme/Attachment"),
            ("Consumer", "java/util/function/Consumer"),
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        let resolver = MapResolver { members, simple };

        // getName IS on Attachment → the correctly-typed lambda param resolves → no false positive.
        let ok = "class C { String result; Svc svc; void m() { svc.consume(result -> result.getName()); } }";
        let d: Vec<String> = unknown_members(ok, &resolver).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "target-typed lambda param should resolve getName on Attachment, got {d:?}");

        // A method NOT on Attachment IS flagged — proving `result` typed to Attachment (not skipped,
        // and not the `String` field, which also lacks `nope`).
        let bad = "class C { String result; Svc svc; void m() { svc.consume(result -> result.nope()); } }";
        let d2: Vec<String> = unknown_members(bad, &resolver).into_iter().map(|x| x.message).collect();
        assert_eq!(d2.len(), 1, "nope() is not on Attachment → one diagnostic, got {d2:?}");
        assert!(d2[0].contains("nope"), "{d2:?}");
    }
}

#[cfg(test)]
mod method_reference_tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;

    struct Res(HashMap<String, ClassMembers>);
    impl TypeResolver for Res {
        fn members_of(&self, b: &str) -> Option<std::sync::Arc<ClassMembers>> {
            self.0.get(b).cloned().map(std::sync::Arc::new)
        }
        fn resolve_simple_name(&self, n: &str, _i: &[Import]) -> Option<String> {
            (n == "Util").then(|| "p/Util".to_string())
        }
        fn is_project_type(&self, b: &str) -> bool {
            self.0.contains_key(b)
        }
    }

    fn resolver() -> Res {
        let mut m = HashMap::new();
        m.insert(
            "p/Util".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![Member::method(
                    "validazioneStandard",
                    TypeRef::simple("boolean"),
                    Vec::new(),
                )],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        Res(m)
    }

    fn run(src: &str) -> Vec<String> {
        unknown_members(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_method_reference_to_something_that_exists_is_silent() {
        let out = run("package p;\nclass A { Object f() { return Util::validazioneStandard; } }\n");
        assert!(out.is_empty(), "{out:?}");
    }

    /// What a half-applied rename leaves: the declaration moved, the `::` site did not.
    #[test]
    fn a_method_reference_to_a_gone_method_is_reported() {
        let out = run("package p;\nclass A { Object f() { return Util::validazione_standard; } }\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].contains("validazione_standard"), "{out:?}");
    }

    /// A constructor reference names no method, and an unresolvable qualifier stays silent.
    #[test]
    fn constructor_references_and_unknown_qualifiers_are_left_alone() {
        assert!(run("package p;\nclass A { Object f() { return Util::new; } }\n").is_empty());
        assert!(run("package p;\nclass A { Object f() { return Mystery::whatever; } }\n").is_empty());
    }
}
