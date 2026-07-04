//! Argument-count diagnostics — a `recv.method(a, b, c)` or `new Foo(a, b)` whose argument count
//! matches **no** overload of that method/constructor. Purely about arity (not argument *types* —
//! that's a separate, harder check), which makes it safe: boxing, generics and widening never change
//! how many arguments a call has.
//!
//! Conservative to the bone (docs: never a false "cannot resolve"):
//!   * only checked when the receiver type is inferred AND its whole hierarchy is resolvable — an
//!     un-indexed supertype could hide the matching overload, so we bail;
//!   * only when at least one overload of that name exists (a *missing* method is
//!     [`crate::members`]'s job, not ours — no double report);
//!   * a trailing array parameter is treated as possibly-varargs (we can't see `ACC_VARARGS` through
//!     the seam), so a varargs call is never mis-flagged.

use bennu_java::prelude::{
    extract_symbols, infer_receiver_type_at, FileSymbols, MemberKind, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// One overload's arity shape: its parameter count and whether the last parameter is an array
/// (hence *maybe* varargs).
#[derive(Clone, Copy)]
struct Sig {
    params: usize,
    last_is_array: bool,
}

impl Sig {
    /// Whether a call with `argc` arguments could bind to this overload.
    fn accepts(&self, argc: usize) -> bool {
        if argc == self.params {
            return true;
        }
        // Possible varargs: `foo(T... xs)` binds 0..=∞ trailing args, so argc may be params-1 or more.
        self.last_is_array && self.params >= 1 && argc + 1 >= self.params
    }
}

/// Parse `source` and flag calls / constructions whose argument count fits no overload.
pub fn arity_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    arity_errors_in(tree.root_node(), source, &symbols, resolver)
}

/// Tree-driven core: reuses the caller's `root` + `symbols` (one parse per file, not per site).
pub fn arity_errors_in(
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
        match n.kind() {
            "method_invocation" => check_call(n, &root, source, bytes, symbols, resolver, &mut out),
            "object_creation_expression" => check_new(n, source, bytes, symbols, resolver, &mut out),
            _ => {}
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
    // Only `receiver.method(...)` — a bare `foo()` resolves against `this`, whose source type the
    // resolver may not fully carry (arity would be unreliable). Aligns with `members`.
    if n.child_by_field_name("object").is_none() {
        return;
    }
    let Some(name) = n.child_by_field_name("name") else { return };
    let Some(args) = n.child_by_field_name("arguments") else { return };
    if name.has_error() || args.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };
    let Some(ty) = infer_receiver_type_at(root, source, symbols, name.start_byte(), resolver) else {
        return;
    };
    if ty.binary_name.is_empty() || !hierarchy_fully_known(resolver, &ty.binary_name) {
        return;
    }

    let sigs = method_overloads(resolver, &ty.binary_name, method);
    if sigs.is_empty() {
        return; // unknown method → members.rs handles it
    }
    let argc = arg_count(args);
    if !sigs.iter().any(|s| s.accepts(argc)) {
        out.push(Diagnostic {
            message: format!(
                "No overload of `{method}` in `{}` takes {argc} argument{}",
                simple_name(&ty.binary_name),
                plural(argc)
            ),
            severity: "error".to_string(),
            start: name.start_byte(),
            end: args.end_byte(),
        });
    }
}

fn check_new(
    n: Node,
    _source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // `new Foo(...)` — skip anonymous-class creations (`new Runnable(){…}`): the args bind to the
    // *supertype's* constructor and the body complicates it. A `class_body` child marks those.
    let Some(ty_node) = n.child_by_field_name("type") else { return };
    let Some(args) = n.child_by_field_name("arguments") else { return };
    if args.has_error() {
        return;
    }
    let mut cw = n.walk();
    for c in n.named_children(&mut cw) {
        if c.kind() == "class_body" {
            return; // anonymous class → skip
        }
    }
    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    let Some(binary) = crate::resolve::type_binary(type_text, symbols, resolver) else { return };

    // Constructors are NOT inherited — look only at this class's own `<init>` methods.
    let Some(cm) = resolver.members_of(&binary) else { return };
    let sigs: Vec<Sig> = cm
        .methods
        .iter()
        .filter(|m| m.name == "<init>" && m.kind == MemberKind::Method)
        .map(sig_of)
        .collect();
    if sigs.is_empty() {
        return; // index may omit constructors → can't assert anything
    }
    let argc = arg_count(args);
    if !sigs.iter().any(|s| s.accepts(argc)) {
        out.push(Diagnostic {
            message: format!(
                "No constructor of `{}` takes {argc} argument{}",
                simple_name(&binary),
                plural(argc)
            ),
            severity: "error".to_string(),
            start: ty_node.start_byte(),
            end: args.end_byte(),
        });
    }
}

/// Collect the arity shapes of every method named `name` reachable from `binary` (walking supertypes).
fn method_overloads(resolver: &dyn TypeResolver, binary: &str, name: &str) -> Vec<Sig> {
    let mut sigs = Vec::new();
    for_each_supertype(resolver, binary, &mut |_bn, cm| {
        for m in &cm.methods {
            if m.name == name && m.kind == MemberKind::Method {
                sigs.push(sig_of(m));
            }
        }
    });
    sigs
}

fn sig_of(m: &bennu_java::prelude::Member) -> Sig {
    Sig {
        params: m.params.len(),
        last_is_array: m.params.last().is_some_and(|p| p.binary_name.ends_with("[]")),
    }
}

fn arg_count(args: Node) -> usize {
    let mut c = args.walk();
    let mut n = 0;
    for _ in args.named_children(&mut c) {
        n += 1;
    }
    n
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef, Visibility};
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

    /// `Svc` with `run()`, `add(int)`, `add(int,int)`, `varargs(String...)`; ctor `Svc(int)`.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "com/acme/Svc".to_string(),
            ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![
                    method("<init>", &["int"]),
                    method("run", &[]),
                    method("add", &["int"]),
                    method("add", &["int", "int"]),
                    method("varargs", &["java/lang/String[]"]),
                ],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method("toString", &[])],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        let simple = [("Svc", "com/acme/Svc")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { members, simple }
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        arity_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn correct_arity_is_ok() {
        assert!(diags("Svc s = null; s.add(1); s.add(1, 2); s.run();").is_empty());
    }

    #[test]
    fn wrong_arity_is_flagged() {
        let d = diags("Svc s = null; s.add(1, 2, 3);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("add") && d[0].contains("3 arguments"), "{d:?}");
    }

    #[test]
    fn zero_args_to_a_one_arg_method_is_flagged() {
        let d = diags("Svc s = null; s.add();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("0 arguments"), "{d:?}");
    }

    #[test]
    fn varargs_accepts_any_trailing_count() {
        assert!(diags("Svc s = null; s.varargs(); s.varargs(\"a\"); s.varargs(\"a\", \"b\");").is_empty());
    }

    #[test]
    fn unknown_method_is_not_arity_flagged() {
        // `nope` doesn't exist → members.rs reports it, arity stays silent (no double count).
        assert!(diags("Svc s = null; s.nope(1, 2, 3);").is_empty());
    }

    #[test]
    fn constructor_arity_is_checked() {
        assert!(diags("Svc s = new Svc(1);").is_empty());
        let d = diags("Svc s = new Svc(1, 2);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("constructor") && d[0].contains("Svc"), "{d:?}");
    }

    #[test]
    fn unknown_receiver_is_not_flagged() {
        assert!(diags("Unknown u = null; u.whatever(1, 2, 3);").is_empty());
    }
}
