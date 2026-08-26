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

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeRef, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::members::simple_name;
use crate::walk::{hierarchy_fully_known, reaches};

/// Parse `source` and flag arguments of the wrong type.
pub fn argument_type_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    argument_type_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` + reuses `root` + `symbols` + inference `cache`.
pub fn argument_type_errors_in(
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
        if n.kind() == "method_invocation" {
            check_call(n, &root, source, bytes, symbols, resolver, cache, &mut out);
        }
    }
    out
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
    // only `receiver.method(...)`, like arity/members
    let Some(obj) = n.child_by_field_name("object") else { return };
    let Some(name) = n.child_by_field_name("name") else { return };
    let Some(arg_list) = n.child_by_field_name("arguments") else { return };
    if name.has_error() || arg_list.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };
    let Some(ty) = infer_node_type_cached(root, source, symbols, &obj, resolver, cache) else {
        return;
    };
    if ty.binary_name.is_empty() {
        return;
    }
    // Shared memoized hierarchy walk (see `InferCache::resolve_methods`): `complete` is the
    // hierarchy-fully-known gate, and the candidates are the overload set (one walk per call site).
    let res = cache.resolve_methods(resolver, &ty.binary_name, method);
    if !res.complete {
        return;
    }
    let args: Vec<Node> = named_args(arg_list);
    let argc = args.len();

    // Every overload of matching ARITY, deduped by parameter list — INCLUDING the ones we can't
    // type-check (varargs / generic). A non-checkable arity match MUST still count: if the call could
    // bind to it, judging the args against a *different*, single checkable overload is unsound. E.g.
    // `setRecipients(String, Addresses[])` + `setRecipients(String, String)`: passing an `Addresses[]`
    // as the 2nd arg binds to the array overload, so we must NOT flag it against `String`. Two arity-2
    // candidates → ambiguous → skip. (Before, the array overload was dropped as non-checkable, leaving
    // the `String` one as the lone signature → false positive.)
    let mut sigs: Vec<&Vec<TypeRef>> = Vec::new();
    for m in &res.candidates {
        // A candidate can bind this call if its arity matches exactly, OR it is varargs (a trailing
        // array parameter) and the call supplies at least its fixed prefix — SLF4J's `debug(String,
        // Object...)` binds a 4-argument `debug(fmt, a, b, c)`. Both shapes MUST enter the ambiguity
        // set: committing to a lone fixed-arity overload (`debug(Marker, String, Object, Object)`) while
        // a varargs overload could also bind is exactly what produced a false "wrong argument type".
        let admits = m.params.len() == argc
            || (m.params.last().is_some_and(|p| p.binary_name.ends_with("[]"))
                && argc + 1 >= m.params.len());
        if admits && !sigs.iter().any(|p| **p == m.params) {
            sigs.push(&m.params);
        }
    }
    // Exactly one overload of this arity, and it must be fully type-checkable (no varargs / generic
    // parameter) for us to bind the arguments to it with certainty. Otherwise → skip.
    let [params] = sigs.as_slice() else { return };
    let params: &Vec<TypeRef> = params;
    if !params_checkable(params) {
        return;
    }

    for (i, arg) in args.iter().enumerate() {
        let Some(param) = params.get(i) else { break };
        let Some(arg_ty) = infer_node_type_cached(root, source, symbols, arg, resolver, cache)
        else {
            continue;
        };
        if let Some((a, p)) = arg_mismatch(&arg_ty.binary_name, param, resolver) {
            out.push(crate::check_id::CheckId::ArgumentType.at(
                *arg,
                format!(
                    "Argument {} of `{method}`: `{a}` cannot be passed where `{p}` is expected",
                    i + 1
                ),
            ));
        }
    }
}

/// A parameter list we can type-check: none is a type variable (generic) or an array (possible
/// varargs / element inference we don't model).
fn params_checkable(params: &[TypeRef]) -> bool {
    params.iter().all(|p| !is_type_var(&p.binary_name) && !p.binary_name.ends_with("[]"))
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
    // Same SIMPLE name, different binaries → almost always the SAME logical type resolved to two
    // different binary FORMS: a nested type spelled `Outer/Inner` (from a source FQN) vs `Outer$Inner`
    // (from bytecode), or two files resolving the simple name through different packages. Passing a
    // value where the SAME type is expected is legal, so the "`ComunicazioneType` cannot be passed
    // where `ComunicazioneType` is expected" report is a false positive → don't flag (sound: at worst
    // a missed genuine same-simple-name mismatch across packages, which is rare and low-value).
    if simple_name(arg) == simple_name(pbin) {
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
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member};
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
        let params = params.iter().map(|p| TypeRef::simple(p.to_string())).collect();
        Member::method(name, TypeRef::simple("void"), params)
    }

    fn cls(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
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
        // A DIFFERENT type sharing the simple name `Widget` (another package) — for the same-name skip.
        members.insert("com/other/Widget".to_string(), cls(vec![]));
        members.insert(
            "com/acme/Svc".to_string(),
            cls(vec![
                method("label", &["java/lang/String", "java/lang/String"]),
                method("take", &["com/acme/Animal"]),
                method("overloaded", &["int"]),
                method("overloaded", &["java/lang/String"]),
                method("animal", &[]), // returns Animal below via return_type override
                // Two arity-2 overloads, ONE with an array param (non-checkable): a call must not be
                // judged against the lone checkable `(String, String)` — mirrors the reported
                // `setRecipients(String, Addresses[])` + `setRecipients(String, String)` case.
                method("recip", &["java/lang/String", "com/acme/Widget[]"]),
                method("recip", &["java/lang/String", "java/lang/String"]),
                // A VARARGS overload of a DIFFERENT arity than a fixed sibling — SLF4J's
                // `debug(String, Object...)` vs `debug(Marker, String, Object, Object)`. A 3-arg call
                // could bind the varargs, so the fixed arity-3 must not be judged alone.
                method("emit", &["java/lang/String", "java/lang/Object[]"]),
                method("emit", &["com/acme/Widget", "java/lang/String", "java/lang/String"]),
                // A parameter typed as a SAME-SIMPLE-NAME type in another package (`com/other/Widget`
                // vs the argument's `com/acme/Widget`) — the same-name-collision case.
                method("dup", &["com/other/Widget"]),
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
    fn overload_with_array_param_sibling_is_skipped() {
        // `recip` has `(String, Widget[])` and `(String, String)`. Passing a non-array `Widget` as the
        // 2nd arg would, before the fix, be judged against the lone checkable `(String, String)` and
        // flagged Widget↔String — but the call could bind to the array overload, so we must skip.
        assert!(diags("s.recip(\"a\", s.widget());").is_empty());
        // And a genuinely correct call still passes.
        assert!(diags("s.recip(\"a\", \"b\");").is_empty());
    }

    #[test]
    fn unknown_receiver_is_skipped() {
        assert!(diags("Unknown u = null; u.whatever(1);").is_empty());
    }

    #[test]
    fn same_simple_name_argument_is_not_flagged() {
        // `dup(com.other.Widget)` called with a `com.acme.Widget` — same simple name, different
        // packages. Very likely one logical type resolved through two packages; the
        // "`Widget` cannot be passed where `Widget` is expected" message is unhelpful → never flagged.
        assert!(diags("s.dup(s.widget());").is_empty());
    }

    #[test]
    fn varargs_overload_of_other_arity_is_skipped() {
        // `emit(String, Object...)` (varargs) can bind a 3-argument call, so the arity-3
        // `emit(Widget, String, String)` must NOT be judged alone (that flagged `"a"` ↔ Widget). This
        // is the SLF4J `LOG.debug("fmt", id, name, note)` false positive, where the call binds the
        // varargs but the check committed to the arity-4 `debug(Marker, String, Object, Object)`.
        assert!(diags("s.emit(\"a\", s.widget(), \"c\");").is_empty());
    }
}
