//! Argument-**type** diagnostics — an argument whose type can't be passed to the corresponding
//! parameter (`foo("a", "b", "c")` called `foo(1, …)`). The type counterpart of [`crate::calls::arity`]
//! (which only counts arguments).
//!
//! Four call shapes are read: `recv.method(args)` — a value's method, `this.m(…)`, `super.m(…)`, or a
//! static `Util.m(…)` (see [`crate::support::resolve::call_receiver_binary`]) —, a **bare** `method(args)`
//! (receiver = the implicit `this`, see [`crate::support::bare_call`]), and `new Foo(args)`, anonymous bodies
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
//!     admits is refused at some position on evidence of its own ([`crate::support::assignable`]) — the shared rules
//!     abstain in their own way, and a report must stand on this module's;
//!   * a position is judged only where the parameter's type is concrete: never a type variable
//!     (of the method or of its owner — the rest of a generic method is judged), never an array, and
//!     never at or past a trailing array, which may be varargs;
//!   * an argument is reported when every admitted overload refuses THAT position, naming each
//!     expected type; when the overloads are refused at different positions, the call is reported.
//!     Method references and untyped arguments are never reported; `null` only where a primitive is
//!     expected, a lambda only where the parameter is provably no functional interface of its arity.

mod ambiguity;
#[cfg(test)]
mod tests;

use bennu_java::prelude::{
    infer_node_type_cached, lambda_refused, overload_fit, same_binary_type, FileSymbols, InferCache,
    Member, OverloadFit, TypeRef, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::calls::lambda_body;
use crate::support::bare_call::{bare_call_scope, index_covers_file_sigs, BareCalls, MemberOwner};
use crate::support::constructors::constructor_call;
use crate::engine::check_id::CheckId;
use crate::support::nodes::{is_primitive, simple_name};
use crate::support::assignable::{definite_mismatch, null_mismatch, spells_its_type, Mismatch};

/// Parse `source` and flag arguments of the wrong type.
pub fn argument_type_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::engine::check::collect_nodes(root);
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
            "object_creation_expression" | "explicit_constructor_invocation" => {
                check_constructor(n, &file, &mut out)
            }
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
        !self.declares(binary)
            || index_covers_file_sigs(binary, name, candidates, self.symbols, self.resolver)
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

/// A bare `method(a, b)`: the candidate set is the one of the innermost class that has a method of
/// that name — a nested or anonymous class, the top type — or, when none does, what the static
/// imports bring in (JLS §15.12.1).
fn check_bare_call(n: Node, bare: &BareCalls, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    let Some(name) = n.child_by_field_name("name") else { return };
    let Ok(method) = name.utf8_text(file.bytes()) else { return };
    let owner = match bare.member_owner(n, file.bytes(), file.symbols, file.resolver) {
        Some(MemberOwner::Nested(owner)) => owner,
        Some(MemberOwner::Top) => bare.top_binary.clone(),
        None => return,
    };
    let res = file.cache.resolve_methods(file.resolver, &owner, method);
    if !res.complete {
        return;
    }
    // A non-empty member set shadows every static import of the name.
    if !res.candidates.is_empty() {
        if file.overloads_current(&owner, method, &res.candidates) {
            judge_args(&res.candidates, n, name, Callee::Method(method), file, out);
        }
        return;
    }
    // Nothing of the name in the class — unless the file declares one the index has not seen yet.
    if owner != bare.top_binary || !bare.file_sigs(method).is_empty() {
        return;
    }
    if let Some(imported) = bare.static_import_methods(method, file.resolver) {
        judge_args(&imported, n, name, Callee::Method(method), file, out);
    }
}

/// `new Foo(a, b)`, `super(a, b)` or `this(a, b)` — one type's own constructors, see
/// [`constructor_call`].
fn check_constructor(n: Node, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    let Some(call) = constructor_call(n, file.bytes(), file.symbols, file.resolver) else { return };
    if !file.overloads_current(&call.binary, "<init>", &call.candidates) {
        return;
    }
    let callee = Callee::Constructor(simple_name(&call.binary));
    judge_args(&call.candidates, n, call.head, callee, file, out);
}

/// `receiver.method(a, b)` — on a value, `this`, `super` or a type (a static call).
fn check_call(n: Node, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    let Some(name) = n.child_by_field_name("name") else { return };
    if name.has_error() {
        return;
    }
    let Ok(method) = name.utf8_text(file.bytes()) else { return };
    let Some(receiver) = crate::support::resolve::call_receiver_binary(
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
    let receiver_type = n
        .child_by_field_name("object")
        .and_then(|obj| infer_node_type_cached(&file.root, file.source, file.symbols, &obj, file.resolver, file.cache))
        .filter(|ty| same_binary_type(&ty.binary_name, &receiver))
        .unwrap_or_else(|| TypeRef::simple(receiver.clone()));
    let candidates = through_receiver(&receiver_type, method, &res.candidates, file.resolver);
    judge_args(&candidates, n, name, Callee::Method(method), file, out);
}

/// The overloads of `method` with each parameter read through the arguments `receiver` gives the
/// class that declares it — `List<String>.add(E)` is `add(String)`, and a `StringBox extends
/// Box<String>` gives `Box.set(T)` a `String` too.
///
/// A type variable is left standing — and then fits anything — where substituting would guess: the
/// method declares a type parameter of that name itself, the receiver is raw, or the argument is a
/// captured wildcard, whose variance the index does not keep (`add(1)` is fine on a
/// `List<? super Integer>` and not on a `List<? extends Number>`). `candidates` comes back unchanged
/// when the hierarchy cannot be walked the same way twice.
fn through_receiver(receiver: &TypeRef, method: &str, candidates: &[Member], resolver: &dyn TypeResolver) -> Vec<Member> {
    let mut read: Vec<Member> = Vec::new();
    // The erased shape of each method kept, so the same method met again further up the hierarchy
    // — an interface's `sort(Comparator<? super E>)` after `ArrayList`'s override of it, each written
    // in its own type variables — stays one candidate instead of becoming two that look ambiguous.
    let mut seen: Vec<Vec<(String, u8)>> = Vec::new();
    let walked = bennu_java::prelude::walk::<()>(resolver, receiver, |a| {
        let class_params = &a.members.type_params;
        let args = &a.ty.type_args;
        for m in a.members.methods.iter().filter(|m| m.name == method) {
            let shape = erased_shape(&m.params, class_params, &m.method_type_params());
            if seen.contains(&shape) {
                continue;
            }
            seen.push(shape);
            let mut member = m.clone();
            if !args.is_empty() && args.len() == class_params.len() {
                let own = m.method_type_params();
                let visible: Vec<String> =
                    class_params.iter().map(|p| if own.contains(p) { String::new() } else { p.clone() }).collect();
                member.params = m
                    .params
                    .iter()
                    .map(|p| {
                        let substituted = bennu_java::prelude::substitute(p, &visible, args);
                        if substituted.names_a_wildcard() { p.clone() } else { substituted }
                    })
                    .collect();
            }
            read.push(member);
        }
        None
    });
    if !walked.complete || read.is_empty() {
        return candidates.to_vec();
    }
    read
}

/// A parameter list with every type variable — the class's or the method's — read as one unknown, so
/// two declarations of the same method written in different variables compare equal.
fn erased_shape(params: &[TypeRef], class_params: &[String], own: &[String]) -> Vec<(String, u8)> {
    params
        .iter()
        .map(|p| {
            let variable = class_params.contains(&p.binary_name) || own.contains(&p.binary_name);
            let name = if variable { "?".to_string() } else { p.binary_name.clone() };
            (name, p.dims)
        })
        .collect()
}

/// The shared decision: when no overload of `candidates` can take `call`'s arguments, and each one
/// the count admits is refused at some position on this module's own evidence, report where.
/// `head` (the method name, or the constructed type) starts the range of a whole-call report.
/// The lambdas and method references passed to `method`, each judged against the functional
/// interface of the parameter it binds to — see [`lambda_body`]. A varargs tail is not read.
fn functional_arguments(method: &Member, args: &[Node], callee: Callee, file: &FileCtx, out: &mut Vec<Diagnostic>) {
    if args.len() != method.params.len() {
        return;
    }
    let ctx = lambda_body::Ctx {
        root: &file.root,
        source: file.source,
        symbols: file.symbols,
        resolver: file.resolver,
        cache: file.cache,
    };
    for (i, (arg, param)) in args.iter().zip(&method.params).enumerate() {
        let is_lambda = match arg.kind() {
            "lambda_expression" => true,
            "method_reference" => false,
            _ => continue,
        };
        let Some(sam) = lambda_body::sam_through(param, file.resolver) else { continue };
        let problem = match is_lambda {
            true => lambda_body::lambda_body_mismatch(*arg, &sam, &ctx),
            false => lambda_body::method_reference_mismatch(*arg, &sam, &ctx),
        };
        if let Some(problem) = problem {
            out.push(CheckId::ArgumentType.at(*arg, format!("Argument {} of `{}`: {problem}", i + 1, callee.name())));
        }
    }
}
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
    let fit = overload_fit(
        &file.root,
        file.source,
        file.symbols,
        &call,
        candidates,
        file.resolver,
        file.cache,
    );
    let args = named_args(arg_list);
    let admitted = match fit {
        OverloadFit::Inapplicable(admitted) => admitted,
        // Several survivors: an error only when they are provably ambiguous — see `ambiguity`. The
        // arguments are typed here and not before, so a call that binds pays for no second look.
        OverloadFit::Applicable(kept) if kept.len() > 1 => {
            let judged: Vec<Judged> = args.iter().map(|a| judged_arg(*a, file)).collect();
            if ambiguity::provably_ambiguous(&kept, &judged, file.resolver) {
                out.push(CheckId::ArgumentType.span(
                    head.start_byte(),
                    arg_list.end_byte(),
                    ambiguity::message(callee.name(), &kept),
                ));
            }
            return;
        }
        // One binding: the call is settled, so a lambda or method reference among the arguments can
        // be read against the parameter it binds to.
        OverloadFit::Applicable(kept) => {
            if let [bound] = kept.as_slice() {
                functional_arguments(bound, &args, callee, file, out);
            }
            return;
        }
        OverloadFit::NoArity => return,
    };
    let judged: Vec<Judged> = args.iter().map(|a| judged_arg(*a, file)).collect();
    let refusals: Vec<Vec<Option<Mismatch>>> = admitted
        .iter()
        .map(|m| position_mismatches(m, &judged, file))
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

/// An argument as one position is judged.
enum Judged<'t> {
    /// Typed by inference; `written` when the argument spells that type itself — see
    /// [`spells_its_type`].
    Typed { ty: TypeRef, written: bool },
    /// The `null` literal: it fits any reference and no primitive.
    Null,
    /// A lambda: refused only where the parameter is provably no functional interface of its arity.
    Lambda(Node<'t>),
    /// A method reference, or an expression inference could not type.
    Unjudged,
}

/// Per argument position, the definite mismatch against overload `m` — `None` where the position is
/// fine, untyped, or not one this module judges.
fn position_mismatches(m: &Member, args: &[Judged], file: &FileCtx) -> Vec<Option<Mismatch>> {
    // A trailing array may be varargs (the seam carries no `ACC_VARARGS`). When the count spreads
    // arguments over it, nothing from its index on is certain; with exactly one argument there, that
    // argument may be the whole array or its single element, and only a refusal of both is.
    let varargs_at = m.params.last().filter(|p| p.is_array()).map(|_| m.params.len() - 1);
    let spread = args.len() != m.params.len();
    args.iter()
        .enumerate()
        .map(|(i, arg)| {
            if spread && varargs_at.is_some_and(|v| i >= v) {
                return None;
            }
            let param = m.params.get(i)?;
            let refused = judged_mismatch(arg, param, file)?;
            if varargs_at == Some(i) {
                let mut element = param.clone();
                element.dims = element.dims.saturating_sub(1);
                judged_mismatch(arg, &element, file)?;
            }
            Some(refused)
        })
        .collect()
}

fn judged_mismatch(arg: &Judged, param: &TypeRef, file: &FileCtx) -> Option<Mismatch> {
    match arg {
        Judged::Typed { ty, written } => definite_mismatch(ty, param, *written, file.resolver),
        Judged::Null => null_mismatch(param),
        Judged::Lambda(lambda) => {
            lambda_refused(&file.root, file.source, file.symbols, lambda, param, file.resolver, file.cache)
                .then(|| Mismatch { found: "lambda".to_string(), expected: rendered(param) })
        }
        Judged::Unjudged => None,
    }
}

/// A parameter type as a message names it: a primitive as written, a class by its simple name.
fn rendered(param: &TypeRef) -> String {
    let name = param.binary_name.as_str();
    param.with_brackets(if is_primitive(name) { name } else { simple_name(name) })
}

/// What an argument is judged as — never by a method reference's type, which it takes from the
/// parameter rather than bring to it.
fn judged_arg<'t>(arg: Node<'t>, file: &FileCtx) -> Judged<'t> {
    match arg.kind() {
        "null_literal" => Judged::Null,
        "lambda_expression" => Judged::Lambda(arg),
        "method_reference" => Judged::Unjudged,
        _ => match infer_node_type_cached(&file.root, file.source, file.symbols, &arg, file.resolver, file.cache) {
            Some(ty) => Judged::Typed { ty, written: spells_its_type(arg) },
            None => Judged::Unjudged,
        },
    }
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
