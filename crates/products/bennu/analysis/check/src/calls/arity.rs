//! Argument-count diagnostics — a `recv.method(a, b, c)` or `new Foo(a, b)` whose argument count
//! matches **no** overload of that method/constructor. Purely about arity (not argument *types* —
//! that's a separate, harder check), which makes it safe: boxing, generics and widening never change
//! how many arguments a call has.
//!
//! Two shapes are read: `recv.method(…)`, whose receiver gives the type to ask — a value, or the type
//! of a static call (`Util.convert(…)`, see [`crate::support::resolve::call_receiver_binary`]) — and a **bare**
//! `method(…)`, whose receiver is the implicit `this` — see [`crate::support::bare_call`] for the guards that
//! make naming `this` safe. The bare one is the shape a class calling its own methods is made of,
//! and it went unjudged for as long as the check only looked for a receiver. A bare call inside a
//! lambda is judged too: a lambda adds no methods.
//!
//! Conservative to the bone (docs: never a false "cannot resolve"):
//!   * only checked when the receiver type is inferred AND its whole hierarchy is resolvable — an
//!     un-indexed supertype could hide the matching overload, so we bail;
//!   * only when at least one overload of that name exists (a *missing* method is
//!     [`crate::calls::members`]'s job, not ours — no double report);
//!   * a trailing array parameter is treated as possibly-varargs (we can't see `ACC_VARARGS` through
//!     the seam), so a varargs call is never mis-flagged.

use bennu_java::prelude::{extract_symbols, FileSymbols, InferCache, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::support::nodes::simple_name;

/// One overload's arity shape: its parameter count and whether the last parameter is an array
/// (hence *maybe* varargs).
#[derive(Clone, Copy)]
struct Sig {
    params: usize,
    last_is_array: bool,
}

impl Sig {
    /// Whether a call with `argc` arguments could bind to this overload — the shared count rule
    /// (`bennu_java`'s `arity_admits`), the same one overload applicability starts from. A varargs
    /// flag on an empty parameter list cannot happen, and is not allowed to widen anything if it does.
    fn accepts(&self, argc: usize) -> bool {
        bennu_java::prelude::arity_admits(self.params, self.last_is_array && self.params >= 1, argc)
    }
}

/// Parse `source` and flag calls / constructions whose argument count fits no overload.
pub fn arity_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::engine::check::collect_nodes(root);
    arity_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` + reuses `root` + `symbols` + inference `cache`.
pub fn arity_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    // Built once per file, not per call: it walks the static-import owners' hierarchies and gathers
    // every signature the file declares. `None` means some whole-file guard failed → no bare call in
    // this file is judgeable, and the receiver-ful ones carry on unaffected.
    let bare = crate::support::bare_call::bare_call_scope(root, source, symbols, resolver);
    for &n in nodes {
        match n.kind() {
            "method_invocation" => {
                check_call(n, &root, source, bytes, symbols, resolver, cache, &mut out);
                if let Some(bare) = &bare {
                    check_bare_call(n, bare, bytes, resolver, cache, &mut out);
                }
            }
            "object_creation_expression" => check_new(n, source, bytes, symbols, resolver, &mut out),
            _ => {}
        }
    }
    out
}

/// A bare `method(a, b)` — the receiver is `this`, so the overload set is the top type's, plus every
/// signature the file itself declares (which the index may not have seen yet).
fn check_bare_call(
    n: Node,
    bare: &crate::support::bare_call::BareCalls,
    bytes: &[u8],
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    // Across lambdas: a lambda declares no methods and does not rebind `this`, so the overload set
    // is the same inside one as outside — and counting arguments never needs their types, which is
    // the only thing the stricter scope protects. Refusing lambdas left every call to an own method
    // from a callback (`opt.ifPresent(h -> own(h))`, a reactive filter's `map(r -> …)`) unjudged.
    let Some(method) = bare.judgeable_across_lambdas(n, bytes) else { return };
    let Some(name) = n.child_by_field_name("name") else { return };
    let Some(args) = n.child_by_field_name("arguments") else { return };

    let res = cache.resolve_methods(resolver, &bare.top_binary, method);
    if !res.complete {
        return;
    }
    let mut sigs: Vec<Sig> = res.candidates.iter().map(sig_of).collect();
    // The buffer's own declarations, ahead of the index. A `T...` parameter reaches us as written,
    // so varargs is read off the text rather than from a resolved array binary name.
    for fs in bare.file_sigs(method) {
        sigs.push(Sig { params: fs.param_texts.len(), last_is_array: fs.varargs });
    }
    if sigs.is_empty() {
        return; // no such method at all → `unresolved_call`'s finding, not a wrong arity
    }
    let argc = arg_count(args);
    if !sigs.iter().any(|s| s.accepts(argc)) {
        out.push(crate::engine::check_id::CheckId::WrongArgumentCount.span(
            name.start_byte(),
            args.end_byte(),
            format!("No overload of `{method}` takes {argc} argument{}", plural(argc)),
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
    // Only `receiver.method(...)` — a bare `foo()` is `check_bare_call`'s, behind its own guards.
    if n.child_by_field_name("object").is_none() {
        return;
    }
    let Some(name) = n.child_by_field_name("name") else { return };
    let Some(args) = n.child_by_field_name("arguments") else { return };
    if name.has_error() || args.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(bytes) else { return };
    // A value's type, or the type a static call names (`Util.convert(…)`).
    let Some(receiver) =
        crate::support::resolve::call_receiver_binary(n, root, source, symbols, resolver, cache)
    else {
        return;
    };
    // Shared memoized hierarchy walk (see `InferCache::resolve_methods`) — `complete` is the
    // hierarchy-fully-known gate, the candidates are the overload set (no separate walk per call).
    let res = cache.resolve_methods(resolver, &receiver, method);
    if !res.complete {
        return;
    }
    if res.candidates.is_empty() {
        return; // unknown method → members.rs handles it
    }
    let sigs: Vec<Sig> = res.candidates.iter().map(sig_of).collect();
    let argc = arg_count(args);
    if !sigs.iter().any(|s| s.accepts(argc)) {
        out.push(crate::engine::check_id::CheckId::WrongArgumentCount.span(
            name.start_byte(),
            args.end_byte(),
            format!(
                "No overload of `{method}` in `{}` takes {argc} argument{}",
                simple_name(&receiver),
                plural(argc)
            ),
        ));
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
    if args.has_error() || crate::support::nodes::is_qualified_creation(n) {
        return;
    }
    let mut cw = n.walk();
    for c in n.named_children(&mut cw) {
        if c.kind() == "class_body" {
            return; // anonymous class → skip
        }
    }
    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    let Some(binary) = crate::support::resolve::type_binary(type_text, symbols, resolver) else { return };

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
        out.push(crate::engine::check_id::CheckId::WrongArgumentCount.span(
            ty_node.start_byte(),
            args.end_byte(),
            format!(
                "No constructor of `{}` takes {argc} argument{}",
                simple_name(&binary),
                plural(argc)
            ),
        ));
    }
}

fn sig_of(m: &bennu_java::prelude::Member) -> Sig {
    Sig {
        params: m.params.len(),
        last_is_array: m.params.last().is_some_and(|p| p.is_array()),
    }
}

/// The number of ACTUAL arguments in an `argument_list` — tree-sitter exposes `line_comment` /
/// `block_comment` as NAMED children, so a comment between arguments (`f(a, /* x */ b)`, or a
/// commented-out arg) must not be counted or it inflates the arity into a false "too many arguments".
fn arg_count(args: Node) -> usize {
    let mut c = args.walk();
    args.named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
        .count()
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
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
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
        let params = params.iter().map(|p| TypeRef::simple(p.to_string())).collect();
        Member::method(name, TypeRef::simple("void"), params)
    }

    /// `Svc` with `run()`, `add(int)`, `add(int,int)`, `varargs(String...)`; ctor `Svc(int)`.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "com/acme/Svc".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method("toString", &[])],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        // The class the test sources are written in, so a BARE call has a fully-known `this` to bind
        // against. `helper(int)` is the indexed overload; a test that declares another in the source
        // exercises the buffer-ahead-of-index path.
        members.insert(
            "C".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("java/lang/Object")),
                interfaces: Vec::new(),
                methods: vec![method("helper", &["int"])],
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        let simple = [("Svc", "com/acme/Svc"), ("C", "C")]
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

    // ── bare calls (receiver = the implicit `this`) ─────────────────────────────

    #[test]
    fn bare_call_with_the_right_arity_is_ok() {
        assert!(diags("helper(1);").is_empty());
    }

    #[test]
    fn bare_call_with_the_wrong_arity_is_flagged() {
        let d = diags("helper(1, 2);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("helper") && d[0].contains("2 arguments"), "{d:?}");
    }

    #[test]
    fn bare_call_to_a_method_only_the_buffer_declares_is_not_flagged() {
        // `fresh` is in the source but not in the resolver — the index has not caught up. Its arity
        // comes off the CST, so the call is judged against the truth rather than reported missing.
        let src = "class C { void fresh(int a, int b) {} void m() { fresh(1, 2); } }";
        assert!(arity_errors(src, &resolver()).is_empty());
    }

    #[test]
    fn bare_call_of_an_unknown_name_is_not_arity_flagged() {
        // No candidate at all → `unresolved_call` reports it; arity must not double-report.
        assert!(diags("nothingLikeThis(1, 2, 3);").is_empty());
    }

    #[test]
    fn bare_call_inside_a_nested_type_is_not_flagged() {
        // A nested class can declare its own `helper` that the top type's hierarchy knows nothing of.
        let src = "class C { void m() {} class Inner { void go() { helper(1, 2, 3); } } }";
        assert!(arity_errors(src, &resolver()).is_empty());
    }

    fn file_diags(src: &str) -> Vec<String> {
        arity_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    /// The report, verbatim: a private snake_case method whose parameters are `final`, one per line
    /// with a leading comma, called with an enum constant that nothing here can type.
    fn insert_header_src(call: &str) -> String {
        format!(
            "class C {{\n\
             \x20   void run() {{\n\
             \x20       {call}\n\
             \x20   }}\n\
             \x20   private void insert_header_opt(\n\
             \x20           final HttpRequest request\n\
             \x20           , final Headers header\n\
             \x20       ) {{\n\
             \x20       RequestInfo.opt_header(header)\n\
             \x20           .ifPresent(h -> request.getHeaders().add(header.header_name(), h));\n\
             \x20   }}\n\
             }}\n"
        )
    }

    #[test]
    fn the_reported_call_missing_an_argument_is_flagged() {
        let d = file_diags(&insert_header_src("insert_header_opt(Headers.TRACE_ID);"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("insert_header_opt") && d[0].contains("1 argument"), "{d:?}");
    }

    #[test]
    fn the_reported_call_with_both_arguments_is_ok() {
        let src = insert_header_src("insert_header_opt(null, Headers.TRACE_ID);");
        assert!(file_diags(&src).is_empty(), "{:?}", file_diags(&src));
    }

    #[test]
    fn the_reported_call_inside_a_lambda_is_flagged() {
        let d = file_diags(&insert_header_src(
            "java.util.Optional.of(1).ifPresent(x -> insert_header_opt(Headers.TRACE_ID));",
        ));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("insert_header_opt"), "{d:?}");
    }

    #[test]
    fn an_overload_taking_one_argument_is_ok() {
        let src = "class C {\n\
                   void run() { own(Headers.TRACE_ID); }\n\
                   private void own(final Object request, final Headers header) {}\n\
                   private void own(final Headers header) {}\n\
                   }";
        assert!(file_diags(src).is_empty(), "{:?}", file_diags(src));
    }

    #[test]
    fn a_varargs_method_of_the_file_accepts_any_trailing_count() {
        let src = "class C {\n\
                   void run() { log_all(); log_all(\"a\"); log_all(\"a\", \"b\", \"c\"); }\n\
                   private void log_all(\n\
                   \x20   final String... parts\n\
                   ) {}\n\
                   }";
        assert!(file_diags(src).is_empty(), "{:?}", file_diags(src));
    }

    #[test]
    fn an_unresolvable_supertype_silences_bare_calls() {
        // The unknown base could declare `own(int)` — an inherited overload binds a bare call just as
        // an own one does, `private` or not (JLS §15.12.2 searches every member method of that name).
        let mut r = resolver();
        r.members.insert(
            "D".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some(TypeRef::simple("com/missing/Base")),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        r.simple.insert("D".to_string(), "D".to_string());
        let src = "class D extends Base { private void own(int a, int b) {} void m() { own(1); } }";
        assert!(arity_errors(src, &r).is_empty());
    }

    #[test]
    fn a_bare_call_inside_a_lambda_is_judged() {
        let d = diags("Runnable r = () -> helper(1, 2);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("helper"), "{d:?}");
        assert!(diags("Runnable r = () -> helper(1); Runnable q = () -> { helper(2); };").is_empty());
    }

    #[test]
    fn a_bare_call_inside_an_anonymous_class_is_still_skipped() {
        // The anonymous body could declare its own `helper(int, int)`.
        assert!(diags("Runnable r = new Runnable() { public void run() { helper(1, 2); } };").is_empty());
    }

    #[test]
    fn a_call_on_this_is_judged() {
        let d = diags("this.helper(1, 2);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("helper"), "{d:?}");
        assert!(diags("this.helper(1);").is_empty());
    }

    #[test]
    fn a_static_method_of_the_file_is_judged() {
        let src = "class C { static void s_helper(int a, int b) {} static void sm() { s_helper(1); } }";
        let d = file_diags(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("s_helper"), "{d:?}");
    }

    #[test]
    fn a_statically_imported_name_is_not_judged() {
        // Its owner's overloads are not enumerated here.
        let src = "import static com.acme.Util.util_call;\nclass C { void m() { util_call(1, 2, 3); } }";
        assert!(file_diags(src).is_empty(), "{:?}", file_diags(src));
    }
}
