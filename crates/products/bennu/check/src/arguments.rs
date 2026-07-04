//! Argument-**type** diagnostics — an argument whose type can't be passed to the corresponding
//! parameter (`foo("a", "b", "c")` called `foo(1, …)`). The type counterpart of [`crate::arity`]
//! (which only counts arguments).
//!
//! Overload resolution is hard, so this is deliberately narrow and conservative (never a false
//! positive):
//!   * only `recv.method(args)` with an inferred receiver whose whole hierarchy is resolvable;
//!   * only when there is **exactly one** candidate overload (same name + arity) after dedup, and it
//!     is neither varargs nor generic (a type-variable parameter) — otherwise we can't be sure which
//!     signature binds, so we skip;
//!   * a single argument is flagged only for a **definite** mismatch: a `String` ↔ primitive pair, or
//!     two unrelated concrete classes (the argument isn't a subtype of the parameter). Boxing,
//!     widening, interfaces, generics and `null` are all treated as OK.

use bennu_java::prelude::{infer_expression_type_at, infer_receiver_type_at, FileSymbols, MemberKind, Member, TypeRef, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::walk::{for_each_supertype, hierarchy_fully_known, reaches};

/// Parse `source` and flag arguments of the wrong type.
pub fn argument_type_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    argument_type_errors_in(tree.root_node(), source, &symbols, resolver)
}

/// Tree-driven core: reuses the caller's `root` + `symbols`.
pub fn argument_type_errors_in(
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
    if n.child_by_field_name("object").is_none() {
        return; // only `receiver.method(...)`, like arity/members
    }
    let Some(name) = n.child_by_field_name("name") else { return };
    let Some(arg_list) = n.child_by_field_name("arguments") else { return };
    if name.has_error() || arg_list.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };
    let Some(ty) = infer_receiver_type_at(root, source, symbols, name.start_byte(), resolver) else {
        return;
    };
    if ty.binary_name.is_empty() || !hierarchy_fully_known(resolver, &ty.binary_name) {
        return;
    }
    let args: Vec<Node> = named_args(arg_list);

    // Candidate overloads: same name + arity, not varargs, not generic. If exactly one distinct
    // signature survives, we know which parameters the arguments bind to.
    let sigs = candidate_signatures(resolver, &ty.binary_name, method, args.len());
    let [params] = sigs.as_slice() else { return };

    for (i, arg) in args.iter().enumerate() {
        let Some(param) = params.get(i) else { break };
        let Some(arg_ty) = infer_expression_type_at(root, source, symbols, arg.start_byte(), arg.end_byte(), resolver)
        else {
            continue;
        };
        if let Some((a, p)) = arg_mismatch(&arg_ty.binary_name, param, resolver) {
            out.push(Diagnostic {
                message: format!(
                    "Argument {} of `{method}`: `{a}` cannot be passed where `{p}` is expected",
                    i + 1
                ),
                severity: "error".to_string(),
                start: arg.start_byte(),
                end: arg.end_byte(),
            });
        }
    }
}

/// Distinct parameter-type lists of the overloads of `name` with `argc` parameters that we can check
/// (non-varargs, non-generic). Deduped so an inherited/overridden identical signature counts once.
fn candidate_signatures(
    resolver: &dyn TypeResolver,
    binary: &str,
    name: &str,
    argc: usize,
) -> Vec<Vec<TypeRef>> {
    let mut sigs: Vec<Vec<TypeRef>> = Vec::new();
    for_each_supertype(resolver, binary, &mut |_bn, cm| {
        for m in &cm.methods {
            if m.name == name && m.kind == MemberKind::Method && m.params.len() == argc && checkable(m) {
                let params = m.params.clone();
                if !sigs.contains(&params) {
                    sigs.push(params);
                }
            }
        }
    });
    sigs
}

/// A method whose parameters we can type-check: none is a type variable (generic) or an array
/// (possible varargs / element inference we don't model).
fn checkable(m: &Member) -> bool {
    m.params.iter().all(|p| !is_type_var(&p.binary_name) && !p.binary_name.ends_with("[]"))
}

/// A definite argument/parameter mismatch, or `None` when compatible / uncertain.
fn arg_mismatch(arg: &str, param: &TypeRef, resolver: &dyn TypeResolver) -> Option<(String, String)> {
    let pbin = param.binary_name.as_str();
    // String ↔ primitive, either direction.
    if is_primitive(pbin) && arg == "java/lang/String" {
        return Some(("String".to_string(), pbin.to_string()));
    }
    if pbin == "java/lang/String" && is_primitive(arg) {
        return Some((arg.to_string(), "String".to_string()));
    }
    // Boxing / widening / unbxoing — treat as OK.
    if is_primitive(arg) || is_primitive(pbin) {
        return None;
    }
    // A reference parameter that's an interface / Object → skip (the argument may implement it).
    if pbin == "java/lang/Object" {
        return None;
    }
    let Some(pcm) = resolver.members_of(pbin) else { return None };
    if pcm.flags.is_interface {
        return None;
    }
    // Both concrete classes, argument hierarchy known: the argument must be a subtype of the parameter.
    if is_type_var(arg) || arg.ends_with("[]") || resolver.members_of(arg).is_none() {
        return None;
    }
    if !hierarchy_fully_known(resolver, arg) {
        return None;
    }
    (!reaches(resolver, arg, pbin))
        .then(|| (simple_name(arg).to_string(), simple_name(pbin).to_string()))
}

fn named_args(arg_list: Node) -> Vec<Node> {
    let mut c = arg_list.walk();
    arg_list
        .named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Visibility};
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
        fn resolve_simple_name(&self, name: &str, _i: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn method(name: &str, params: &[&str]) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple("void"),
            params: params.iter().map(|p| TypeRef::simple(p.to_string())).collect(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            visibility: Visibility::Public,
            raw_signature: name.to_string(),
        }
    }

    fn cls(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// `Svc` with `label(String,String)`, `take(Animal)`, `overloaded(int)` + `overloaded(String)`.
    /// Animal / Dog (Dog extends Animal) / Widget (unrelated).
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cls(vec![]));
        members.insert("java/lang/String".to_string(), cls(vec![]));
        members.insert("com/acme/Animal".to_string(), cls(vec![]));
        let mut dog = cls(vec![]);
        dog.superclass = Some("com/acme/Animal".to_string());
        members.insert("com/acme/Dog".to_string(), dog);
        members.insert("com/acme/Widget".to_string(), cls(vec![]));
        members.insert(
            "com/acme/Svc".to_string(),
            cls(vec![
                method("label", &["java/lang/String", "java/lang/String"]),
                method("take", &["com/acme/Animal"]),
                method("overloaded", &["int"]),
                method("overloaded", &["java/lang/String"]),
                method("animal", &[]), // returns Animal below via return_type override
            ]),
        );
        // Give Svc providers returning types, for building args.
        if let Some(svc) = members.get_mut("com/acme/Svc") {
            svc.methods.push({
                let mut m = method("dog", &[]);
                m.return_type = TypeRef::simple("com/acme/Dog");
                m
            });
            svc.methods.push({
                let mut m = method("widget", &[]);
                m.return_type = TypeRef::simple("com/acme/Widget");
                m
            });
        }
        let simple = [
            ("Svc", "com/acme/Svc"),
            ("Animal", "com/acme/Animal"),
            ("Dog", "com/acme/Dog"),
            ("Widget", "com/acme/Widget"),
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ Svc s; void m() {{ {body} }} }}");
        argument_type_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn int_for_string_param_is_flagged() {
        let d = diags("s.label(1, \"b\");");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Argument 1") && d[0].contains("String"), "{d:?}");
    }

    #[test]
    fn correct_string_args_are_ok() {
        assert!(diags("s.label(\"a\", \"b\");").is_empty());
    }

    #[test]
    fn subtype_argument_is_ok() {
        // take(Animal) with a Dog → OK.
        assert!(diags("s.take(s.dog());").is_empty());
    }

    #[test]
    fn unrelated_class_argument_is_flagged() {
        let d = diags("s.take(s.widget());");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Widget") && d[0].contains("Animal"), "{d:?}");
    }

    #[test]
    fn ambiguous_overload_is_skipped() {
        // `overloaded` has two distinct 1-arg signatures → we don't guess which binds.
        assert!(diags("s.overloaded(1);").is_empty());
        assert!(diags("s.overloaded(\"x\");").is_empty());
    }

    #[test]
    fn unknown_receiver_is_skipped() {
        assert!(diags("Unknown u = null; u.whatever(1);").is_empty());
    }
}
