//! Argument-**type** diagnostics — an argument whose type can't be passed to the corresponding
//! parameter (`foo("a", "b", "c")` called `foo(1, …)`). The type counterpart of [`crate::arity`]
//! (which only counts arguments).
//!
//! Four call shapes are read: `recv.method(args)` — a value's method, `this.m(…)`, `super.m(…)`, or a
//! static `Util.m(…)` (see [`crate::resolve::call_receiver_binary`]) —, a **bare** `method(args)`
//! (receiver = the implicit `this`, see [`crate::bare_call`]), and `new Foo(args)`, anonymous bodies
//! included: `new Foo(args) { … }` passes its arguments to `Foo`'s own constructors (JLS §15.9.5.1).
//!
//! Which overloads a call could bind to is NOT decided here: it is `bennu_java`'s `overload_fit`, the
//! one applicability implementation go-to, hover, inference and parameter hints use (arity with
//! varargs, strict / loose / varargs phases, lambdas against functional interfaces, `null`, boxing,
//! subtyping — abstaining on anything the classpath cannot read). This check only turns a proven
//! "nothing applies" into a message, and stays narrow on top of it (never a false positive):
//!   * only a call whose candidate set is complete: a receiver whose whole hierarchy is resolvable,
//!     the enclosing type for a bare call, or a resolvable type for a `new` — and, for a type this
//!     file declares, an index that has already seen every signature the buffer declares;
//!   * only when the shared rules find **no** applicable overload, and **every** overload the count
//!     admits is refused at some position on evidence of its own ([`mismatch`]) — the shared rules
//!     abstain in their own way, and a report must stand on this module's;
//!   * a position is judged only where the parameter's type is concrete: never a type variable
//!     (of the method or of its owner — the rest of a generic method is judged), never an array, and
//!     never at or past a trailing array, which may be varargs;
//!   * an argument is reported when every admitted overload refuses THAT position, naming each
//!     expected type; when the overloads are refused at different positions, the call is reported.
//!     Lambdas, method references, `null` and untyped arguments are never reported.

mod mismatch;
#[cfg(test)]
mod tests;

use bennu_java::prelude::{
    infer_node_type_cached, overload_fit, same_binary_type, FileSymbols, InferCache, Member,
    MemberKind, OverloadFit, TypeRef, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::bare_call::{bare_call_scope, index_covers_file_sigs, BareCalls};
use crate::check_id::CheckId;
use crate::nodes::{is_type_var, simple_name};
use mismatch::{arg_mismatch, Mismatch};

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
    let file = FileCtx { root, source, symbols, resolver, cache };
    let bare = bare_call_scope(root, source, symbols, resolver);
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_invocation" if n.child_by_field_name("object").is_some() => {
                check_call(n, &file, &mut out)
            }
            "method_invocation" => {
                if let Some(bare) = &bare {
                    check_bare_call(n, bare, &file, &mut out);
                }
            }
            "object_creation_expression" => check_new(n, &file, &mut out),
            _ => {}
        }
    }
    out
}

/// Everything a judgement reads about the file, passed as one.
struct FileCtx<'a, 't> {
    root: Node<'t>,
    source: &'a str,
    symbols: &'a FileSymbols,
    resolver: &'a dyn TypeResolver,
    cache: &'a InferCache,
}

impl FileCtx<'_, '_> {
    fn bytes(&self) -> &[u8] {
        self.source.as_bytes()
    }

    /// Whether this file declares `binary` — the types whose index entry can lag behind the buffer.
    fn declares(&self, binary: &str) -> bool {
        self.symbols
            .types
            .iter()
            .any(|t| same_binary_type(&t.fqn.replace('.', "/"), binary))
    }

    /// `candidates` of `binary` under `name` are the whole overload set: trivially for a type of
    /// another file, and for one of this file only once the index has seen every signature the
    /// buffer declares — a method typed a moment ago would otherwise leave a stale set standing.
    fn overloads_current(&self, binary: &str, name: &str, candidates: &[Member]) -> bool {
        !self.declares(binary) || index_covers_file_sigs(name, candidates, self.symbols, self.resolver)
    }
}

/// What a call calls, for its messages.
#[derive(Clone, Copy)]
enum Callee<'a> {
    Method(&'a str),
    Constructor(&'a str),
}

impl Callee<'_> {
    fn name(&self) -> &str {
        match self {
            Callee::Method(n) | Callee::Constructor(n) => n,
        }
    }

    fn none_accepts(&self) -> String {
        match self {
            Callee::Method(n) => format!("No overload of `{n}` accepts these arguments"),
            Callee::Constructor(n) => format!("No constructor of `{n}` accepts these arguments"),
        }
    }
}

/// A bare `method(a, b)`: the candidate set is the enclosing type's.
fn check_bare_call(n: Node, bare: &BareCalls, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    let Some(method) = bare.judgeable_member_call(n, file.bytes()) else { return };
    let Some(name) = n.child_by_field_name("name") else { return };
    let res = file.cache.resolve_methods(file.resolver, &bare.top_binary, method);
    // `judgeable_member_call`'s contract: act only on a non-empty member set, which is what shadows
    // every static import of the name.
    if !res.complete || res.candidates.is_empty() {
        return;
    }
    if !file.overloads_current(&bare.top_binary, method, &res.candidates) {
        return;
    }
    judge_args(&res.candidates, n, name, Callee::Method(method), file, out);
}

/// `new Foo(a, b)` — the candidates are `Foo`'s own constructors (never inherited). An anonymous
/// body changes nothing: its implicit constructor forwards to the one of `Foo` the arguments select.
fn check_new(n: Node, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    let Some(ty_node) = n.child_by_field_name("type") else { return };
    let Ok(type_text) = ty_node.utf8_text(file.bytes()) else { return };
    let Some(binary) = crate::resolve::type_binary(type_text, file.symbols, file.resolver) else {
        return;
    };
    let Some(cm) = file.resolver.members_of(&binary) else { return };
    let ctors: Vec<Member> = cm
        .methods
        .iter()
        .filter(|m| m.name == "<init>" && m.kind == MemberKind::Method)
        .cloned()
        .collect();
    if ctors.is_empty() {
        return; // an index that omits constructors can assert nothing
    }
    if !file.overloads_current(&binary, "<init>", &ctors) {
        return;
    }
    judge_args(&ctors, n, ty_node, Callee::Constructor(simple_name(&binary)), file, out);
}

/// `receiver.method(a, b)` — on a value, `this`, `super` or a type (a static call).
fn check_call(n: Node, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    let Some(name) = n.child_by_field_name("name") else { return };
    if name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(file.bytes()) else { return };
    let Some(receiver) = crate::resolve::call_receiver_binary(
        n,
        &file.root,
        file.source,
        file.symbols,
        file.resolver,
        file.cache,
    ) else {
        return;
    };
    // Shared memoized hierarchy walk (see `InferCache::resolve_methods`): `complete` is the
    // hierarchy-fully-known gate, and the candidates are the overload set (one walk per call site).
    let res = file.cache.resolve_methods(file.resolver, &receiver, method);
    if !res.complete || !file.overloads_current(&receiver, method, &res.candidates) {
        return;
    }
    judge_args(&res.candidates, n, name, Callee::Method(method), file, out);
}

/// The shared decision: when no overload of `candidates` can take `call`'s arguments, and each one
/// the count admits is refused at some position on this module's own evidence, report where.
/// `head` (the method name, or the constructed type) starts the range of a whole-call report.
fn judge_args(
    candidates: &[Member],
    call: Node,
    head: Node,
    callee: Callee,
    file: &FileCtx,
    out: &mut Vec<Diagnostic>,
) {
    let Some(arg_list) = call.child_by_field_name("arguments") else { return };
    if arg_list.has_error() {
        return; // a list being edited is not a call to judge
    }
    // Applicable (one overload or several) is not an error, and neither is an arity no overload
    // admits — that is `arity`'s finding. Every overload the count admits is weighed, varargs and
    // generic ones included: `setRecipients(String, Addresses[])` beside `setRecipients(String,
    // String)`, or SLF4J's `debug(String, Object...)` beside a fixed four-parameter `debug`, bind
    // calls a lone checkable signature would have called wrong.
    let OverloadFit::Inapplicable(admitted) = overload_fit(
        &file.root,
        file.source,
        file.symbols,
        &call,
        candidates,
        file.resolver,
        file.cache,
    ) else {
        return;
    };
    let args = named_args(arg_list);
    let arg_types: Vec<Option<TypeRef>> = args.iter().map(|a| judged_arg_type(*a, file)).collect();
    let refusals: Vec<Vec<Option<Mismatch>>> = admitted
        .iter()
        .map(|m| position_mismatches(m, &arg_types, file.resolver))
        .collect();
    if refusals.iter().any(|r| r.iter().all(Option::is_none)) {
        return;
    }
    let before = out.len();
    for (i, arg) in args.iter().enumerate() {
        let at_position: Option<Vec<Mismatch>> = refusals.iter().map(|r| r[i].clone()).collect();
        if let Some(refused) = at_position {
            out.push(CheckId::ArgumentType.at(*arg, argument_message(i, callee, &refused)));
        }
    }
    if out.len() == before {
        out.push(CheckId::ArgumentType.span(
            head.start_byte(),
            arg_list.end_byte(),
            callee.none_accepts(),
        ));
    }
}

/// Per argument position, the definite mismatch against overload `m` — `None` where the position is
/// fine, untyped, or not one this module judges.
fn position_mismatches(
    m: &Member,
    arg_types: &[Option<TypeRef>],
    resolver: &dyn TypeResolver,
) -> Vec<Option<Mismatch>> {
    // A trailing array may be varargs (the seam carries no `ACC_VARARGS`): from its index on, an
    // argument may be an element or the whole array, so nothing there is certain.
    let varargs_from = m.params.last().filter(|p| p.is_array()).map(|_| m.params.len() - 1);
    arg_types
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            if varargs_from.is_some_and(|v| i >= v) {
                return None;
            }
            let param = m.params.get(i)?;
            if param.is_array() || is_type_var(&param.binary_name) {
                return None;
            }
            arg_mismatch(ty.as_ref()?, param, resolver)
        })
        .collect()
}

/// The type an argument is judged by — never a lambda's, a method reference's or `null`'s, which
/// have none of their own.
fn judged_arg_type(arg: Node, file: &FileCtx) -> Option<TypeRef> {
    if matches!(arg.kind(), "lambda_expression" | "method_reference" | "null_literal") {
        return None;
    }
    infer_node_type_cached(&file.root, file.source, file.symbols, &arg, file.resolver, file.cache)
}

/// `Argument 3 of `m`: `Widget` cannot be passed where `Animal` is expected` — with every distinct
/// expected type when several overloads refuse the position.
fn argument_message(i: usize, callee: Callee, refused: &[Mismatch]) -> String {
    let mut expected: Vec<&str> = Vec::new();
    for r in refused {
        if !expected.contains(&r.expected.as_str()) {
            expected.push(&r.expected);
        }
    }
    let found = refused.first().map_or("", |r| r.found.as_str());
    format!(
        "Argument {} of `{}`: `{found}` cannot be passed where `{}` is expected",
        i + 1,
        callee.name(),
        expected.join("` or `")
    )
}

fn named_args(arg_list: Node) -> Vec<Node> {
    let mut c = arg_list.walk();
    arg_list
        .named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
        .collect()
}
