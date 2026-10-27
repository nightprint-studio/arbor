//! Arguments a **generic** signature refuses (resolver-backed, JLS §15.12.2).
//!
//! [`crate::calls::arguments`] judges a position only where the parameter's type is concrete. What it
//! leaves are the positions written in type variables and wildcards — the ones that need to know what
//! the variable stands for. Two families are decided here, each only on evidence that is certain:
//!
//! **A receiver with a wildcard.** For `List<? extends Number> numbers` the type of `E` is unknown, so
//! `numbers.add(1)` has no argument it accepts except `null`; the same holds for `List<?>`, and for
//! `List<? super Integer>` anything that is not an `Integer` is refused. The variance is read from the
//! receiver variable's declared type, since the index collapses a wildcard onto its bound.
//!
//! **A generic method of the project**, read from the source rendering of its signature
//! (`<T extends Number> double num(T value)`):
//!   * an argument for a bare type variable must meet the variable's bounds (`num("x")`,
//!     `maxOf(new Object(), …)` with `T extends Comparable<T>`), judged on the erasure of the bound;
//!   * an argument for `G<? extends B>` must carry an `a` that is a `B` (`sum(List<String>)`), and one
//!     for `G<B>` exactly `B` — generics are invariant (`sumExact(List<Integer>)`);
//!   * a variable fixed by an invariant position (`Map<K, V>` given a `Map<String, Integer>`) fixes the
//!     other positions that use it (`put(counts, "key", "value")`);
//!   * explicit type arguments (`GenericApi.<String>num(…)`) must be as many as the method's variables
//!     and within their bounds.
//!
//! Never judged: a lambda or method reference among the arguments, a varargs candidate, a signature
//! the index holds in class-file form, a type that does not resolve or whose hierarchy has a gap, a
//! captured wildcard, an array, `null`, and any call whose overload set is not the whole set.

use std::collections::HashMap;

use bennu_java::prelude::{
    infer_node_type_cached, same_binary_type, FileSymbols, InferCache, Member, MemberKind, TypeRef, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::source::generics_syntax::type_params_in_scope;
use crate::support::assignable::{box_of, definite_mismatch, spells_its_type};
use crate::support::bare_call::index_covers_file_sigs;
use crate::support::nodes::is_type_var;
use crate::support::resolve::{call_receiver_binary, type_binary, written_type_binary};
use crate::support::scopes::enclosing_executable_scope;
use crate::support::walk::{for_each_supertype, hierarchy_fully_known, reaches};

const OBJECT: &str = "java/lang/Object";

struct File<'a, 't> {
    root: Node<'t>,
    source: &'a str,
    symbols: &'a FileSymbols,
    resolver: &'a dyn TypeResolver,
    cache: &'a InferCache,
}

impl File<'_, '_> {
    fn bytes(&self) -> &[u8] {
        self.source.as_bytes()
    }

    fn infer(&self, node: Node) -> Option<TypeRef> {
        infer_node_type_cached(&self.root, self.source, self.symbols, &node, self.resolver, self.cache)
            .filter(|t| !t.binary_name.is_empty())
    }

    fn declares(&self, binary: &str) -> bool {
        self.symbols.types.iter().any(|t| same_binary_type(&t.fqn.replace('.', "/"), binary))
    }

    /// Resolvable, with no gap anywhere in its hierarchy: "not a subtype" about it is a fact.
    fn is_known(&self, binary: &str) -> bool {
        self.resolver.members_of(binary).is_some() && hierarchy_fully_known(self.resolver, binary)
    }

    /// Whether `from` is certainly not `target` or one of its subtypes.
    fn certainly_not_a(&self, from: &str, target: &str) -> bool {
        target != OBJECT && self.is_known(from) && self.is_known(target) && !reaches(self.resolver, from, target)
    }
}

/// Flag arguments a generic signature or a wildcard receiver refuses.
pub fn generic_argument_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let file = File { root, source, symbols, resolver, cache };
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_invocation" if n.child_by_field_name("object").is_some() => check_call(n, &file, &mut out),
            "local_variable_declaration" => check_declaration(n, &file, &mut out),
            _ => {}
        }
    }
    out
}

fn check_call(call: Node, file: &File, out: &mut Vec<Diagnostic>) {
    let Some(name) = call.child_by_field_name("name").filter(|n| !n.has_error()) else { return };
    let Ok(method) = name.utf8_text(file.bytes()) else { return };
    let Some(arg_list) = call.child_by_field_name("arguments").filter(|a| !a.has_error()) else { return };
    let mut c = arg_list.walk();
    let args: Vec<Node> =
        arg_list.named_children(&mut c).filter(|a| !matches!(a.kind(), "line_comment" | "block_comment")).collect();
    if args.iter().any(|a| matches!(a.kind(), "lambda_expression" | "method_reference")) {
        return;
    }
    if let Some(refused) = wildcard_receiver(call, method, &args, file) {
        out.push(CheckId::ArgumentType.at(
            refused,
            format!("Argument of `{method}` cannot be passed: the receiver's type argument is a wildcard that accepts no such value"),
        ));
        return;
    }
    if let Some(refused) = generic_method(call, name, method, &args, file) {
        out.push(CheckId::ArgumentType.at(
            refused,
            format!("Argument of `{method}` does not fit the generic signature: it is outside the bounds or type arguments the parameter allows"),
        ));
    }
}

// ── family 1: a receiver with a wildcard ─────────────────────────────────────

/// How a wildcard type argument is bounded.
enum Wildcard<'t> {
    /// `?`: the type is unknown. Written to, only `null` fits; read from, it is an `Object`.
    Unbounded,
    /// `? extends X`: unknown too — only `null` can be written — and read from, it is an `X`.
    Extends(Node<'t>),
    /// `? super X`: anything that is an `X` can be written; read from, it is an `Object`.
    Super(Node<'t>),
}

/// A call on a variable declared with a wildcard, resolved to the methods of its class that take these
/// arguments: the class's binary name, its type variables, and its own candidates.
struct WildcardCall<'t> {
    class_params: Vec<String>,
    wildcards: Vec<Option<Wildcard<'t>>>,
    own: Vec<Member>,
}

/// `receiver.method(args)` where the receiver is a variable the enclosing method declares with a
/// wildcard argument, and its class's own `method` is the whole candidate set for that many arguments.
fn wildcard_call<'t>(call: Node<'t>, method: &str, argc: usize, file: &File) -> Option<WildcardCall<'t>> {
    let receiver = call.child_by_field_name("object").filter(|o| o.kind() == "identifier")?;
    let declared = declared_generic_type(receiver, file.bytes())?;
    let wildcards = wildcard_arguments(declared);
    if wildcards.iter().all(Option::is_none) {
        return None;
    }
    let class = written_type_binary(declared, file.bytes(), file.symbols, file.resolver)?;
    let members = file.resolver.members_of(&class)?;
    if members.type_params.len() != wildcards.len() || !file.is_known(&class) {
        return None;
    }
    let (mut own, mut inherited): (Vec<Member>, Vec<Member>) = (Vec::new(), Vec::new());
    for_each_supertype(file.resolver, &class, &mut |owner, cm| {
        for m in cm.methods.iter().filter(|m| m.kind == MemberKind::Method && m.name == method && m.params.len() == argc) {
            if same_binary_type(owner, &class) { own.push(m.clone()) } else { inherited.push(m.clone()) }
        }
    });
    let own_shapes: Vec<Vec<(String, u8)>> = own.iter().map(shape).collect();
    // A same-named method further up with a signature of its own may take the arguments: leave the call.
    if own.is_empty() || inherited.iter().any(|m| !own_shapes.contains(&shape(m))) {
        return None;
    }
    Some(WildcardCall { class_params: members.type_params.clone(), wildcards, own })
}

fn wildcard_receiver<'t>(call: Node<'t>, method: &str, args: &[Node<'t>], file: &File) -> Option<Node<'t>> {
    let WildcardCall { class_params, wildcards, own } = wildcard_call(call, method, args.len(), file)?;
    let in_scope = type_params_in_scope(call, file.bytes());
    let mut reported: Option<Node> = None;
    for m in &own {
        let own_vars = m.method_type_params();
        let refused = (0..args.len()).find(|&j| {
            let Some(k) = variable_index(&m.params[j], &class_params, &own_vars) else { return false };
            match &wildcards[k] {
                Some(Wildcard::Unbounded) | Some(Wildcard::Extends(_)) => args[j].kind() != "null_literal",
                Some(Wildcard::Super(bound)) => super_refuses(args[j], *bound, &in_scope, file),
                None => false,
            }
        })?;
        reported.get_or_insert(args[refused]);
    }
    reported
}

// ── assignments of what a generic call gives back ────────────────────────────

/// `Declared x = value;` where `value` reads from a wildcard (`Integer first = sink.get(0)` with
/// `List<? super Integer> sink`, an `Object`) or is a generic factory whose argument contradicts the
/// declared type arguments (`Optional<String> text = Optional.of(1)`).
fn check_declaration(decl: Node, file: &File, out: &mut Vec<Diagnostic>) {
    let Some(declared) = decl.child_by_field_name("type").filter(|t| matches!(t.kind(), "type_identifier" | "generic_type")) else {
        return;
    };
    let mut c = decl.walk();
    let values: Vec<Node> = decl
        .named_children(&mut c)
        .filter(|d| d.kind() == "variable_declarator")
        .filter_map(|d| d.child_by_field_name("value"))
        .filter(|v| v.kind() == "method_invocation")
        .collect();
    for value in values {
        let message = wildcard_read(declared, value, file).or_else(|| factory_conflict(declared, value, file));
        if let Some(message) = message {
            out.push(CheckId::IncompatibleType.at(value, message));
        }
    }
}

/// A read through a wildcard that gives back a type the declaration cannot hold.
fn wildcard_read(declared: Node, value: Node, file: &File) -> Option<String> {
    if declared.kind() != "type_identifier" {
        return None;
    }
    let method = value.child_by_field_name("name")?.utf8_text(file.bytes()).ok()?;
    let args = value.child_by_field_name("arguments")?;
    let call = wildcard_call(value, method, args.named_child_count(), file)?;
    let [only] = call.own.as_slice() else { return None };
    let own_vars = only.method_type_params();
    let k = variable_index(&only.return_type, &call.class_params, &own_vars)?;
    let (read, shown): (String, &str) = match call.wildcards[k].as_ref()? {
        Wildcard::Unbounded | Wildcard::Super(_) => (OBJECT.to_string(), "Object"),
        Wildcard::Extends(bound) => (plain_class(*bound, file)?, "its upper bound"),
    };
    let target = written_type_binary(declared, file.bytes(), file.symbols, file.resolver)?;
    file.certainly_not_a(&read, &target).then(|| {
        format!("Incompatible types: reading through a wildcard gives {shown}, which cannot be assigned to `{}`", declared.utf8_text(file.bytes()).unwrap_or_default())
    })
}

/// `Optional<String> text = Optional.of(1)`: the factory's type variable is fixed to `String` by the
/// declaration (the type is invariant in it) and the argument must be a `String`.
fn factory_conflict(declared: Node, value: Node, file: &File) -> Option<String> {
    if declared.kind() != "generic_type" {
        return None;
    }
    let declared_class = written_type_binary(declared, file.bytes(), file.symbols, file.resolver)?;
    let mut tc = declared.walk();
    let type_arguments = declared.named_children(&mut tc).find(|ch| ch.kind() == "type_arguments")?;
    let mut ac = type_arguments.walk();
    let declared_args: Vec<Node> = type_arguments.named_children(&mut ac).collect();

    let name = value.child_by_field_name("name")?;
    let method = name.utf8_text(file.bytes()).ok()?;
    let arg_list = value.child_by_field_name("arguments")?;
    let mut vc = arg_list.walk();
    let args: Vec<Node> = arg_list.named_children(&mut vc).filter(|a| !matches!(a.kind(), "line_comment" | "block_comment")).collect();
    if args.iter().any(|a| matches!(a.kind(), "lambda_expression" | "method_reference")) {
        return None;
    }
    let receiver = call_receiver_binary(value, &file.root, file.source, file.symbols, file.resolver, file.cache)?;
    let res = file.cache.resolve_methods(file.resolver, &receiver, method);
    if !res.complete {
        return None;
    }
    let mut exact = res.candidates.iter().filter(|m| m.kind == MemberKind::Method && m.params.len() == args.len());
    let (Some(m), None) = (exact.next(), exact.next()) else { return None };
    if m.params.last().is_some_and(|p| p.is_array()) || !same_binary_type(&m.return_type.binary_name, &declared_class) {
        return None;
    }
    let own_vars = m.method_type_params();
    if m.return_type.type_args.len() != declared_args.len() {
        return None;
    }
    for (returned, written) in m.return_type.type_args.iter().zip(&declared_args) {
        // The variable the declaration fixes: a bare method-level variable against a plain class.
        if returned.dims > 0 || !returned.type_args.is_empty() || !own_vars.contains(&returned.binary_name) {
            continue;
        }
        let Some(fixed) = plain_class(*written, file) else { continue };
        for (param, arg) in m.params.iter().zip(&args) {
            if param.binary_name != returned.binary_name || param.dims > 0 || !param.type_args.is_empty() {
                continue;
            }
            let Some(ty) = judged_type(*arg, file).filter(|t| !t.names_a_wildcard() && !t.is_array()) else { continue };
            if file.certainly_not_a(&boxed_binary(&ty), &fixed) {
                return Some(format!(
                    "Incompatible types: the declaration makes `{}` a `{}`, and the argument is not",
                    returned.binary_name,
                    declared.utf8_text(file.bytes()).unwrap_or_default()
                ));
            }
        }
    }
    None
}

/// The class type variable a parameter is, by index; `None` for anything else or a variable the method
/// itself redeclares.
fn variable_index(param: &TypeRef, class_params: &[String], own_vars: &[String]) -> Option<usize> {
    if param.dims > 0 || !param.type_args.is_empty() || own_vars.contains(&param.binary_name) {
        return None;
    }
    class_params.iter().position(|p| *p == param.binary_name)
}

/// A parameter list with every type variable read as one unknown, so the same method met again in a
/// supertype (written in that type's own variables) compares equal.
fn shape(m: &Member) -> Vec<(String, u8)> {
    m.params
        .iter()
        .map(|p| (if is_type_var(&p.binary_name) { "?".to_string() } else { p.binary_name.clone() }, p.dims))
        .collect()
}

/// Whether `? super bound` refuses `arg`: the argument's type is certainly not a subtype of the bound.
fn super_refuses(arg: Node, bound: Node, in_scope: &[String], file: &File) -> bool {
    let Ok(text) = bound.utf8_text(file.bytes()) else { return false };
    if bound.kind() != "type_identifier" || in_scope.iter().any(|v| v == text) {
        return false;
    }
    let Some(bound_binary) = written_type_binary(bound, file.bytes(), file.symbols, file.resolver) else { return false };
    let Some(ty) = file.infer(arg).filter(|t| !t.names_a_wildcard() && !t.is_array()) else { return false };
    definite_mismatch(&ty, &TypeRef::simple(bound_binary), spells_its_type(arg), file.resolver).is_some()
}

/// The `generic_type` a variable is declared with, when the enclosing method declares the name exactly
/// once — as a parameter, or as a local that comes first.
fn declared_generic_type<'t>(use_site: Node<'t>, bytes: &[u8]) -> Option<Node<'t>> {
    let name = use_site.utf8_text(bytes).ok()?;
    let method = enclosing_executable_scope(use_site).filter(|m| m.kind() != "lambda_expression")?;
    let mut found: Vec<Node> = Vec::new();
    if let Some(params) = method.child_by_field_name("parameters") {
        let mut c = params.walk();
        for p in params.named_children(&mut c).filter(|p| p.kind() == "formal_parameter") {
            if p.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(name) {
                found.extend(p.child_by_field_name("type"));
            }
        }
    }
    let mut stack = vec![method];
    while let Some(n) = stack.pop() {
        if n.kind() == "local_variable_declaration" {
            let mut c = n.walk();
            let declares = n.named_children(&mut c).any(|d| {
                d.kind() == "variable_declarator"
                    && d.child_by_field_name("name").and_then(|id| id.utf8_text(bytes).ok()) == Some(name)
            });
            if declares && n.start_byte() < use_site.start_byte() {
                found.extend(n.child_by_field_name("type"));
            }
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    match found.as_slice() {
        [only] if only.kind() == "generic_type" => Some(*only),
        _ => None,
    }
}

/// Per type argument of `generic`, how it is bounded when it is a wildcard.
fn wildcard_arguments<'t>(generic: Node<'t>) -> Vec<Option<Wildcard<'t>>> {
    let mut c = generic.walk();
    let Some(args) = generic.named_children(&mut c).find(|ch| ch.kind() == "type_arguments") else { return Vec::new() };
    let mut ac = args.walk();
    let list: Vec<Node> = args.named_children(&mut ac).collect();
    list.into_iter()
        .map(|arg| {
            if arg.kind() != "wildcard" {
                return None;
            }
            let mut wc = arg.walk();
            // `super` is a named node of its own in this grammar: the bound is the type after it.
            let bound = arg
                .named_children(&mut wc)
                .find(|ch| !matches!(ch.kind(), "annotation" | "marker_annotation" | "super"));
            let mut tc = arg.walk();
            let is_super = arg.children(&mut tc).any(|ch| ch.kind() == "super");
            match (bound, is_super) {
                (Some(bound), true) => Some(Wildcard::Super(bound)),
                (Some(bound), false) => Some(Wildcard::Extends(bound)),
                (None, _) => Some(Wildcard::Unbounded),
            }
        })
        .collect()
}

// ── family 2: a generic method of the project ────────────────────────────────

/// One type variable of a method and the bounds written on it.
struct TypeVariable {
    name: String,
    bounds: Vec<String>,
}

/// A parameter's type, as far as this check reads it.
enum ParamKind {
    /// A type variable of the method.
    Var(String),
    /// `Base<…>` with each argument read.
    Generic { base: String, args: Vec<TypeArg> },
    Other,
}

enum TypeArg {
    Var(String),
    Exact(String),
    Extends(String),
    Super,
    Unbounded,
}

struct Signature {
    variables: Vec<TypeVariable>,
    params: Vec<String>,
}

fn generic_method<'t>(call: Node<'t>, head: Node<'t>, method: &str, args: &[Node<'t>], file: &File) -> Option<Node<'t>> {
    let receiver = call_receiver_binary(call, &file.root, file.source, file.symbols, file.resolver, file.cache)?;
    let res = file.cache.resolve_methods(file.resolver, &receiver, method);
    if !res.complete {
        return None;
    }
    if file.declares(&receiver) && !index_covers_file_sigs(&receiver, method, &res.candidates, file.symbols, file.resolver) {
        return None;
    }
    // A candidate whose last parameter is an array may be varargs: the count then proves nothing.
    if res.candidates.iter().any(|m| m.params.last().is_some_and(|p| p.is_array())) {
        return None;
    }
    let exact: Vec<&Member> =
        res.candidates.iter().filter(|m| m.kind == MemberKind::Method && m.params.len() == args.len()).collect();
    if exact.is_empty() {
        return None;
    }
    let explicit = explicit_type_arguments(call);
    let mut reported: Option<Node> = None;
    for m in exact {
        let signature = signature_of(m)?;
        let refused = refusal(&signature, &explicit, args, head, file)?;
        reported.get_or_insert(refused);
    }
    reported
}

fn explicit_type_arguments<'t>(call: Node<'t>) -> Vec<Node<'t>> {
    let Some(list) = call.child_by_field_name("type_arguments") else { return Vec::new() };
    let mut c = list.walk();
    let args: Vec<Node> = list.named_children(&mut c).collect();
    args
}

/// The node a signature refuses, if it refuses these arguments; `head` stands for a refusal of the call.
fn refusal<'t>(sig: &Signature, explicit: &[Node<'t>], args: &[Node<'t>], head: Node<'t>, file: &File) -> Option<Node<'t>> {
    let mut bindings: HashMap<String, String> = HashMap::new();
    if !sig.variables.is_empty() && !explicit.is_empty() {
        if explicit.len() != sig.variables.len() {
            return Some(head);
        }
        for (variable, written) in sig.variables.iter().zip(explicit) {
            let binary = plain_class(*written, file)?;
            if variable.bounds.iter().any(|b| violates_bound(&binary, b, &sig.variables, file)) {
                return Some(*written);
            }
            bindings.insert(variable.name.clone(), binary);
        }
    }
    let kinds: Vec<ParamKind> = sig.params.iter().map(|p| classify(p, &sig.variables)).collect();
    let judged: Vec<Option<TypeRef>> = args.iter().map(|a| judged_type(*a, file)).collect();

    // What the invariant positions fix: `Map<K, V>` given a `Map<String, Integer>` makes K a String.
    for (kind, ty) in kinds.iter().zip(&judged) {
        let (ParamKind::Generic { base, args: patterns }, Some(ty)) = (kind, ty) else { continue };
        if !is_class(base, ty, file) || ty.type_args.len() != patterns.len() {
            continue;
        }
        for (pattern, actual) in patterns.iter().zip(&ty.type_args) {
            if let (TypeArg::Var(v), false) = (pattern, actual.names_a_wildcard() || actual.is_array()) {
                // Two positions that fix the same variable differently: leave the call to javac's lub.
                let conflicts = bindings.get(v).is_some_and(|prior| !same_binary_type(prior, &actual.binary_name));
                if conflicts {
                    return None;
                }
                bindings.insert(v.clone(), actual.binary_name.clone());
            }
        }
    }

    for (j, (kind, ty)) in kinds.iter().zip(&judged).enumerate() {
        let Some(ty) = ty else { continue };
        let refused = match kind {
            ParamKind::Var(v) => {
                let own = sig.variables.iter().find(|tv| tv.name == *v);
                let boxed = boxed_binary(ty);
                let out_of_bound = own.is_some_and(|tv| tv.bounds.iter().any(|b| violates_bound(&boxed, b, &sig.variables, file)));
                let against_binding = bindings.get(v).is_some_and(|bound| file.certainly_not_a(&boxed, bound));
                !ty.names_a_wildcard() && !ty.is_array() && (out_of_bound || against_binding)
            }
            ParamKind::Generic { base, args: patterns } => generic_refused(base, patterns, ty, file),
            ParamKind::Other => false,
        };
        if refused {
            return Some(args[j]);
        }
    }
    None
}

/// An argument's type, unless it is `null`, untypable or already a wildcard capture.
fn judged_type(arg: Node, file: &File) -> Option<TypeRef> {
    if arg.kind() == "null_literal" {
        return None;
    }
    file.infer(arg)
}

/// A primitive's box, or the class itself.
fn boxed_binary(ty: &TypeRef) -> String {
    box_of(&ty.binary_name).map_or_else(|| ty.binary_name.clone(), str::to_string)
}

/// The binary name of a written type that is a plain class: no primitive, array or type variable.
fn plain_class(written: Node, file: &File) -> Option<String> {
    if !matches!(written.kind(), "type_identifier" | "scoped_type_identifier") {
        return None;
    }
    let text = written.utf8_text(file.bytes()).ok()?;
    let in_scope = type_params_in_scope(written, file.bytes());
    if in_scope.iter().any(|v| v == text) {
        return None;
    }
    written_type_binary(written, file.bytes(), file.symbols, file.resolver)
}

/// Whether `argument` (a class) is outside the `bound` of a variable, judged on the bound's erasure.
fn violates_bound(argument: &str, bound: &str, variables: &[TypeVariable], file: &File) -> bool {
    let base = bound.split('<').next().unwrap_or("").trim();
    if variables.iter().any(|v| v.name == base) {
        return false;
    }
    let Some(bound_binary) = type_binary(base, file.symbols, file.resolver) else { return false };
    file.certainly_not_a(argument, &bound_binary)
}

/// Whether `ty` is the class the written `base` names.
fn is_class(base: &str, ty: &TypeRef, file: &File) -> bool {
    type_binary(base, file.symbols, file.resolver).is_some_and(|b| same_binary_type(&b, &ty.binary_name))
}

/// `G<…>` given a `G<…>` whose arguments disagree with what the parameter allows.
fn generic_refused(base: &str, patterns: &[TypeArg], ty: &TypeRef, file: &File) -> bool {
    if !is_class(base, ty, file) || ty.type_args.len() != patterns.len() || ty.is_array() {
        return false;
    }
    patterns.iter().zip(&ty.type_args).any(|(pattern, actual)| {
        if actual.names_a_wildcard() || actual.is_array() || !actual.type_args.is_empty() {
            return false;
        }
        match pattern {
            TypeArg::Exact(text) if !text.contains('<') => type_binary(text, file.symbols, file.resolver)
                .is_some_and(|expected| {
                    !same_binary_type(&expected, &actual.binary_name)
                        && file.is_known(&expected)
                        && file.is_known(&actual.binary_name)
                }),
            TypeArg::Extends(text) if !text.contains('<') => type_binary(text, file.symbols, file.resolver)
                .is_some_and(|bound| file.certainly_not_a(&actual.binary_name, &bound)),
            _ => false,
        }
    })
}

/// A candidate's signature as this check reads it: from the source rendering when the index holds one
/// (bounds and parameter shapes included), otherwise from the class-file form, where only the names of
/// the method's type variables are known and so only a variable standing alone as a parameter is
/// described. That is enough for explicit type arguments — `Collections.<Integer>singletonList("x")` —
/// and says nothing about a call without them, since no bound is known to be violated.
fn signature_of(m: &Member) -> Option<Signature> {
    if let Some(sig) = parse_signature(&m.raw_signature).filter(|s| s.params.len() == m.params.len()) {
        return Some(sig);
    }
    let names = m.method_type_params();
    if names.is_empty() {
        return None;
    }
    let params = m
        .params
        .iter()
        .map(|p| {
            let is_variable = p.dims == 0 && p.type_args.is_empty() && names.contains(&p.binary_name);
            if is_variable { p.binary_name.clone() } else { "?".to_string() }
        })
        .collect();
    let variables = names.into_iter().map(|name| TypeVariable { name, bounds: Vec::new() }).collect();
    Some(Signature { variables, params })
}

// ── reading the source rendering of a signature ──────────────────────────────

/// `<T extends Number> double num(T value)` → its variables and parameter types. `None` for the
/// class-file form, a varargs or annotated parameter, or anything that does not read cleanly.
fn parse_signature(raw: &str) -> Option<Signature> {
    let raw = raw.trim();
    let (variables, rest) = match raw.strip_prefix('<') {
        Some(after) => {
            let end = closing_angle(after)?;
            let rest = &after[end + 1..];
            if rest.starts_with('(') {
                return None; // the class-file spelling, `<T:Ljava/lang/Object;>(TT;)TT;`
            }
            (parse_variables(&after[..end]), rest)
        }
        None => (Vec::new(), raw),
    };
    let (open, close) = (rest.find('(')?, rest.rfind(')')?);
    if close < open {
        return None;
    }
    let inner = rest[open + 1..close].trim();
    let params = match inner.is_empty() {
        true => Vec::new(),
        false => split_top_level(inner, ',').into_iter().map(param_type).collect::<Option<Vec<_>>>()?,
    };
    Some(Signature { variables, params })
}

/// The index of the `>` that closes an already-opened `<`.
fn closing_angle(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '<' => depth += 1,
            '>' if depth == 0 => return Some(i),
            '>' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn parse_variables(inner: &str) -> Vec<TypeVariable> {
    split_top_level(inner, ',')
        .into_iter()
        .map(|part| match part.trim().split_once(" extends ") {
            Some((name, bounds)) => TypeVariable {
                name: name.trim().to_string(),
                bounds: split_top_level(bounds, '&').into_iter().map(|b| b.trim().to_string()).collect(),
            },
            None => TypeVariable { name: part.trim().to_string(), bounds: Vec::new() },
        })
        .collect()
}

/// `Type name` → `Type`; `None` for a varargs, annotated or `final` parameter.
fn param_type(param: &str) -> Option<String> {
    let param = param.trim();
    if param.contains('@') || param.contains("...") || param.starts_with("final ") {
        return None;
    }
    let split = param.rfind(char::is_whitespace)?;
    Some(param[..split].trim().to_string())
}

/// `text` split at `separator`, ignoring any inside `<…>` or `(…)`.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0i32, 0usize);
    for (i, c) in text.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            c if c == separator && depth == 0 => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn classify(text: &str, variables: &[TypeVariable]) -> ParamKind {
    let text = text.trim();
    if text.ends_with(']') {
        return ParamKind::Other;
    }
    if let Some(v) = variables.iter().find(|v| v.name == text) {
        return ParamKind::Var(v.name.clone());
    }
    let Some(lt) = text.find('<').filter(|_| text.ends_with('>')) else { return ParamKind::Other };
    let args = split_top_level(&text[lt + 1..text.len() - 1], ',').into_iter().map(|a| type_arg(a.trim(), variables)).collect();
    ParamKind::Generic { base: text[..lt].trim().to_string(), args }
}

fn type_arg(text: &str, variables: &[TypeVariable]) -> TypeArg {
    if text == "?" {
        return TypeArg::Unbounded;
    }
    if let Some(bound) = text.strip_prefix("? extends ") {
        return TypeArg::Extends(bound.trim().to_string());
    }
    if text.starts_with("? super ") {
        return TypeArg::Super;
    }
    match variables.iter().find(|v| v.name == text) {
        Some(v) => TypeArg::Var(v.name.clone()),
        None => TypeArg::Exact(text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_signature_is_read_into_variables_and_parameters() {
        let sig = parse_signature("<T extends Comparable<T>, U> T pick(Map<String, List<T>> map, U other)").unwrap();
        assert_eq!(sig.variables.len(), 2);
        assert_eq!(sig.variables[0].name, "T");
        assert_eq!(sig.variables[0].bounds, ["Comparable<T>"]);
        assert!(sig.variables[1].bounds.is_empty());
        assert_eq!(sig.params, ["Map<String, List<T>>", "U"]);
        assert!(parse_signature("double sum(List<? extends Number> values)").unwrap().variables.is_empty());
    }

    #[test]
    fn the_class_file_form_varargs_and_annotated_parameters_are_not_read() {
        assert!(parse_signature("<T:Ljava/lang/Object;>(TT;)TT;").is_none());
        assert!(parse_signature("void log(String format, Object... args)").is_none());
        assert!(parse_signature("void take(@Nullable String value)").is_none());
    }

    #[test]
    fn parameters_are_classified_by_what_they_say_about_a_variable() {
        let vars = parse_variables("K, V");
        assert!(matches!(classify("K", &vars), ParamKind::Var(v) if v == "K"));
        assert!(matches!(classify("String", &vars), ParamKind::Other));
        assert!(matches!(classify("T[]", &vars), ParamKind::Other));
        match classify("Map<K, ? extends Number>", &vars) {
            ParamKind::Generic { base, args } => {
                assert_eq!(base, "Map");
                assert!(matches!(&args[0], TypeArg::Var(v) if v == "K"));
                assert!(matches!(&args[1], TypeArg::Extends(b) if b == "Number"));
            }
            _ => panic!("expected a generic parameter"),
        }
    }

    #[test]
    fn splitting_ignores_separators_inside_brackets() {
        assert_eq!(split_top_level("A<B, C>, D", ','), ["A<B, C>", " D"]);
        assert_eq!(split_top_level("Number & Comparable<T>", '&'), ["Number ", " Comparable<T>"]);
    }
}
