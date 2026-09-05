//! What the editor draws *around* a call: the signature of the one you are inside, and the names of
//! the arguments you are passing.
//!
//! Two features, one analysis. Both start from the same question — *which method is this call, and
//! which argument am I looking at?* — so they share the resolution and differ only in what they do
//! with the answer:
//!
//!   * [`signature_at`] answers for the caret: the method's rendered signature and which parameter
//!     the caret sits on, for the strip above the line.
//!   * [`inlay_hints`] answers for every call in the file at once: the parameter name in front of
//!     each argument that does not already say what it is, plus the inferred type of a `var`.
//!
//! ## Conservative in the same way the checks are
//!
//! A hint is a claim about the code, drawn as if the compiler had said it, so a wrong one is worse
//! than none: it reads as authoritative and there is nothing to click through to find out it isn't.
//! Every step here abstains rather than guesses — an unresolved receiver, an ambiguous overload, a
//! parameter list whose arity disagrees with the call, all yield nothing.
//!
//! ## Why the names are worth the trouble
//!
//! `transfer(from, to, 500)` is the argument for the whole feature. The reader cannot tell which
//! way the money goes, and neither can the writer a month later; `transfer(source: from, target: to,
//! amount: 500)` is the same line with the answer in it. Java has no named arguments, so this is the
//! only place that information can appear.

use bennu_java::prelude::{
    infer_node_type_cached, extract_symbols, parse_java, FileSymbols, InferCache, Member,
    MemberKind, TypeResolver,
};
use tree_sitter::Node;

use crate::member_text::{named_parameters, render_signature, render_type};

/// The signature of the call the caret is inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    /// The rendered signature — `transfer(String source, String target, long amount)`.
    pub label: String,
    /// `[start, end)` byte ranges within `label`, one per parameter.
    pub params: Vec<(usize, usize)>,
    /// Index into `params` of the argument the caret is on.
    pub active: usize,
    /// Byte offset of the call's opening paren — what the strip is anchored to.
    pub anchor: usize,
    /// Which overload this is, and how many there were, when the name was overloaded.
    pub overload: Option<(usize, usize)>,
}

/// One hint drawn between the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHint {
    /// Byte offset the hint is drawn at.
    pub offset: usize,
    /// The text — `source:` for a parameter name, `: Order` for a type.
    pub label: String,
    /// `true` when the hint belongs in front of what is at `offset` rather than behind it.
    pub before: bool,
}

// ── Signature help ───────────────────────────────────────────────────────────────────────────

/// The signature of the call whose argument list contains `offset`, or `None`.
///
/// `None` covers every uncertainty: the caret is not in an argument list, the receiver cannot be
/// typed, the name resolves to no method, or the overloads cannot be told apart. The strip simply
/// does not appear — which is what it does today, so nothing regresses into a wrong answer.
pub fn signature_at(
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<SignatureHelp> {
    let tree = parse_java(source)?;
    let root = tree.root_node();
    let symbols = extract_symbols(source);
    let cache = InferCache::new();

    let call = enclosing_call(root, offset)?;
    let args = call.child_by_field_name("arguments")?;
    let candidates = call_candidates(&call, &root, source, &symbols, resolver, &cache)?;

    let argc = argument_count(args);
    let active = active_argument(args, source, offset);

    // Prefer an overload whose arity admits where the caret is: with `2` typed you mean the 3-arg
    // one even though `2` alone also fits the 2-arg one.
    let wanted = argc.max(active + 1);
    let admitting: Vec<&Member> = candidates
        .iter()
        .filter(|m| arity_admits(m, wanted))
        .collect();
    let shown: Vec<&Member> = if admitting.is_empty() { candidates.iter().collect() } else { admitting };
    let picked = *shown.first()?;

    // Rendered from the names the member actually carries: a parameter whose name is unknown shows
    // as its type alone (`get_genere(String)`), never as `String arg0` — which would say the
    // parameter is called that, and it is not called anything we know.
    let params: Vec<(String, String)> = named_parameters(picked)
        .into_iter()
        .map(|(ty, name)| (ty, name.unwrap_or_default()))
        .collect();
    let label = render_signature(&picked.name, &params, &render_type(&picked.return_type));
    Some(SignatureHelp {
        params: parameter_spans(&label, &params),
        label,
        active,
        anchor: args.start_byte(),
        overload: (candidates.len() > 1).then_some((0, shown.len())),
    })
}

/// The `[start, end)` span of each rendered parameter inside `label`.
///
/// Found by scanning rather than recomputed, so the spans cannot drift from the text they index —
/// they are located in the very string that will be shown.
fn parameter_spans(label: &str, params: &[(String, String)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let Some(open) = label.find('(') else { return out };
    let mut from = open + 1;
    for (ty, name) in params {
        let text = crate::member_text::render_param(ty, name);
        match label[from..].find(&text) {
            Some(rel) => {
                let start = from + rel;
                out.push((start, start + text.len()));
                from = start + text.len();
            }
            // Unreachable while `render_signature` builds the label — but a span that is merely
            // *probably* right would mark the wrong parameter, so a miss records nothing.
            None => out.push((0, 0)),
        }
    }
    out
}

/// The innermost call whose ARGUMENT LIST contains `offset`.
///
/// The argument list, not the call: with the caret on the receiver of `a.b(x)` you are not inside
/// any argument list, and a strip about `b` would be about a call you have not started typing.
fn enclosing_call<'t>(root: Node<'t>, offset: usize) -> Option<Node<'t>> {
    let mut node = root.descendant_for_byte_range(offset, offset)?;
    loop {
        if matches!(node.kind(), "method_invocation" | "object_creation_expression") {
            if let Some(args) = node.child_by_field_name("arguments") {
                // Strictly inside the parens: at the `(` itself you have not opened the list yet,
                // and at the `)` you have closed it.
                if offset > args.start_byte() && offset < args.end_byte() {
                    return Some(node);
                }
            }
        }
        node = node.parent()?;
    }
}

/// Every method the call could bind to, by name on the resolved receiver.
fn call_candidates(
    call: &Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<Vec<Member>> {
    let bytes = source.as_bytes();
    // `new Foo(…)` binds to a constructor, which the member model keeps under `<init>`.
    if call.kind() == "object_creation_expression" {
        let ty = call.child_by_field_name("type")?;
        let resolved = infer_node_type_cached(root, source, symbols, &ty, resolver, cache)?;
        let cm = resolver.members_of(&resolved.binary_name)?;
        return non_empty(
            cm.methods.iter().filter(|m| m.name == "<init>").cloned().collect(),
        );
    }
    let name_node = call.child_by_field_name("name")?;
    let name = name_node.utf8_text(bytes).ok()?;

    // An unqualified call is on the enclosing type; a qualified one on whatever the receiver is.
    let owner = match call.child_by_field_name("object") {
        Some(recv) => infer_node_type_cached(root, source, symbols, &recv, resolver, cache)?,
        None => {
            let binary = bennu_java::prelude::enclosing_type_binary(source, call.start_byte())?;
            bennu_java::prelude::TypeRef::simple(binary)
        }
    };
    let mut out: Vec<Member> = Vec::new();
    collect_named(resolver, &owner.binary_name, name, &mut out);
    non_empty(out)
}

fn non_empty(v: Vec<Member>) -> Option<Vec<Member>> {
    (!v.is_empty()).then_some(v)
}

/// Gather every method called `name` on `binary` and its supertypes, most-derived first.
fn collect_named(resolver: &dyn TypeResolver, binary: &str, name: &str, out: &mut Vec<Member>) {
    bennu_java::prelude::walk_up::<()>(resolver, &bennu_java::prelude::TypeRef::simple(binary), |a| {
        for m in &a.members.methods {
            if m.kind == MemberKind::Method && m.name == name {
                // An override arrives again from every level it is declared at; the first (most
                // derived) is the one the call binds to, and the shared walk is breadth-first, so
                // that is the one that gets here first.
                if !out.iter().any(|k| k.params == m.params) {
                    out.push(m.clone());
                }
            }
        }
        None
    });
}

/// Whether `m` could take a call of `argc` arguments (a trailing array parameter is varargs).
fn arity_admits(m: &Member, argc: usize) -> bool {
    let n = m.params.len();
    if n == argc {
        return true;
    }
    // A varargs parameter is an ARRAY, which is now `dims` and no longer a suffix on the name.
    let variadic = m.params.last().is_some_and(|p| p.dims > 0);
    variadic && argc + 1 >= n
}

/// How many arguments the list holds — 0 for an empty one.
fn argument_count(args: Node) -> usize {
    let mut c = args.walk();
    args.named_children(&mut c).filter(|n| !is_trivia(*n)).count()
}

/// Which argument `offset` is on: the number of top-level commas before it.
///
/// Counted over the source text rather than the tree because a list being typed is a broken tree —
/// `f(a, ` has no second argument node for the caret to be inside of, and that is exactly the
/// moment the strip has to move the mark along.
fn active_argument(args: Node, source: &str, offset: usize) -> usize {
    let start = args.start_byte() + 1;
    let end = offset.min(args.end_byte());
    if end <= start {
        return 0;
    }
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut chars = source[start..end].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            // Skip the literal wholesale: a comma inside it separates nothing.
            '"' | '\'' => {
                let mut escaped = false;
                for c in chars.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == ch {
                        break;
                    }
                }
            }
            ',' if depth <= 0 => commas += 1,
            _ => {}
        }
    }
    commas
}

fn is_trivia(n: Node) -> bool {
    matches!(n.kind(), "line_comment" | "block_comment")
}

// ── Inlay hints ──────────────────────────────────────────────────────────────────────────────

/// Every inlay hint for `source`: parameter names at call sites, and inferred `var` types.
///
/// Whole-file rather than per-viewport because the analysis is one parse and one memoized inference
/// pass over it — the same one validation makes — and slicing that by scroll position would mean
/// redoing it on every scroll for no saving.
pub fn inlay_hints(source: &str, resolver: &dyn TypeResolver) -> Vec<InlayHint> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let symbols = extract_symbols(source);
    let cache = InferCache::new();
    let mut out = Vec::new();
    walk_hints(root, &root, source, &symbols, resolver, &cache, &mut out);
    out.sort_by_key(|h| h.offset);
    out
}

fn walk_hints(
    node: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<InlayHint>,
) {
    match node.kind() {
        "method_invocation" | "object_creation_expression" => {
            parameter_name_hints(&node, root, source, symbols, resolver, cache, out);
        }
        "local_variable_declaration" => {
            var_type_hint(&node, root, source, symbols, resolver, cache, out);
        }
        "lambda_expression" => {
            lambda_param_hints(&node, root, source, symbols, resolver, cache, out);
        }
        _ => {}
    }
    let mut c = node.walk();
    for child in node.named_children(&mut c) {
        walk_hints(child, root, source, symbols, resolver, cache, out);
    }
}

/// `transfer(source: from, target: to, amount: 500)` — a name in front of each argument that does
/// not already say what it is.
fn parameter_name_hints(
    call: &Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<InlayHint>,
) {
    let Some(args) = call.child_by_field_name("arguments") else { return };
    let bytes = source.as_bytes();
    let mut c = args.walk();
    let arg_nodes: Vec<Node> = args.named_children(&mut c).filter(|n| !is_trivia(*n)).collect();
    if arg_nodes.is_empty() {
        return;
    }
    let Some(candidates) = call_candidates(call, root, source, symbols, resolver, cache) else {
        return;
    };
    // Only an unambiguous binding earns a hint. With two overloads admitting the same call, the
    // names could come from either, and a name from the wrong one is a lie about the code.
    let admitting: Vec<&Member> =
        candidates.iter().filter(|m| arity_admits(m, arg_nodes.len())).collect();
    let [picked] = admitting.as_slice() else { return };
    let params = named_parameters(picked);
    if params.len() != arg_nodes.len() {
        return; // a varargs call, whose tail has no one name
    }
    for (arg, (_, name)) in arg_nodes.iter().zip(params.iter()) {
        // No name, no hint. A class file carries no parameter names unless it was compiled with
        // `-parameters`, and the placeholder the override generator uses — `arg0` — would read here
        // as a claim that the parameter is *called* that. Saying nothing is the true answer.
        let Some(name) = name else { continue };
        let Ok(text) = arg.utf8_text(bytes) else { continue };
        if !worth_naming(text, name) {
            continue;
        }
        out.push(InlayHint {
            offset: arg.start_byte(),
            label: format!("{name}:"),
            before: true,
        });
    }
}

/// Whether an argument written as `text` is worth prefixing with the parameter name `name`.
///
/// The rule is IntelliJ's, and it is the difference between a useful feature and visual noise: an
/// argument that already carries the name says it better than a hint would. `transfer(source, …)`
/// needs nothing; `transfer(a, …)` and `transfer(500, …)` do. A long expression is left alone too —
/// by the time a reader has parsed `repo.find(id).orElseThrow()` they know what it is, and a prefix
/// on it is just more to read.
fn worth_naming(text: &str, name: &str) -> bool {
    const MAX_LEN: usize = 32;
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_LEN {
        return false;
    }
    // A lambda or a method reference reads as its own explanation, and the hint would land in the
    // middle of the arrow.
    if trimmed.contains("->") || trimmed.contains("::") {
        return false;
    }
    // The argument already says the name: `name`, `theName`, `orderName`, `NAME`.
    let lower = trimmed.to_ascii_lowercase();
    let want = name.to_ascii_lowercase();
    if lower == want || lower.ends_with(&want) || want.ends_with(&lower) {
        return false;
    }
    true
}

/// `var order = load();` → `: Order` after the name.
fn var_type_hint(
    decl: &Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<InlayHint>,
) {
    let bytes = source.as_bytes();
    let Some(ty) = decl.child_by_field_name("type") else { return };
    // `val` too: it is Lombok's, the inference engine already resolves it, and a hint that knew
    // only `var` left every Lombok local without the type it had already worked out.
    if !ty.utf8_text(bytes).is_ok_and(bennu_java::prelude::is_inferred_type) {
        return; // a written type needs no hint — it is right there
    }
    let mut c = decl.walk();
    for child in decl.named_children(&mut c) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let (Some(name), Some(value)) =
            (child.child_by_field_name("name"), child.child_by_field_name("value"))
        else {
            continue;
        };
        let Some(inferred) = infer_node_type_cached(root, source, symbols, &value, resolver, cache)
        else {
            continue;
        };
        // A binary name without a slash is one of two very different things, and treating them as
        // one left half the locals in a method unlabelled. A **primitive** is a known type, and the
        // one the reader most often cannot work out: `var n = path.indexOf('/')` is `int`, not
        // `Integer` and not `long`, and nothing on the line says so. An **unresolved** name is a
        // simple name we failed to resolve, and that one still has to stay quiet — a hint is drawn
        // as if the compiler had said it.
        if inferred.binary_name.is_empty() {
            continue;
        }
        if !inferred.binary_name.contains('/')
            && !bennu_java::prelude::is_primitive(&inferred.binary_name)
        {
            continue;
        }
        out.push(InlayHint {
            offset: name.end_byte(),
            label: format!(": {}", render_type(&inferred)),
            before: false,
        });
    }
}

/// `rows.forEach(row: String -> …)` — the type of a lambda parameter that was written without one.
///
/// ## Why this one matters more than the others
///
/// A `var` at least names the expression it was inferred from, two words to the right. An implicit
/// lambda parameter names nothing: `row` is typed by the functional interface the lambda is being
/// passed to, which is in another file, and reading the code tells you only that somebody called it
/// `row`. The engine already resolves it — target-typing a lambda parameter is what
/// `bennu_java`'s `lambda_param` does — so the type was there all along and only the hint was
/// missing.
///
/// A lambda whose parameters ARE written gets nothing: the type is right there.
fn lambda_param_hints(
    lambda: &Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<InlayHint>,
) {
    let Some(params) = lambda.child_by_field_name("parameters") else { return };
    let names: Vec<Node> = match params.kind() {
        // `row -> …`
        "identifier" => vec![params],
        // `(row, index) -> …`
        "inferred_parameters" => {
            let mut c = params.walk();
            params.named_children(&mut c).filter(|n| n.kind() == "identifier").collect()
        }
        // `(String row) -> …` — written, so there is nothing to say.
        _ => return,
    };
    let Some(body) = lambda.child_by_field_name("body") else { return };
    for name in names {
        let text = &source[name.start_byte()..name.end_byte()];
        // Asked of a USE inside the body, not of the parameter's own identifier: the engine types a
        // name by classifying it against the scopes around it, and a declaration is not one of the
        // things it classifies. A parameter the body never reads has nothing to hint anyway.
        let Some(use_node) = first_use(&body, source, text) else { continue };
        let Some(inferred) =
            infer_node_type_cached(root, source, symbols, &use_node, resolver, cache)
        else {
            continue;
        };
        if inferred.binary_name.is_empty() || !inferred.binary_name.contains('/') {
            continue; // unresolved, or a primitive with nothing to add
        }
        out.push(InlayHint {
            offset: name.end_byte(),
            label: format!(": {}", render_type(&inferred)),
            before: false,
        });
    }
}

/// The first identifier inside `body` that reads `name`.
fn first_use<'t>(body: &Node<'t>, source: &str, name: &str) -> Option<Node<'t>> {
    let mut stack = vec![*body];
    let mut best: Option<Node<'t>> = None;
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && &source[n.start_byte()..n.end_byte()] == name {
            if best.is_none_or(|b| n.start_byte() < b.start_byte()) {
                best = Some(n);
            }
        }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The active-argument count is pure text work, so it can be checked without a resolver — and it
    /// is the part that has to keep working on a half-typed list, where there is no tree to ask.
    fn active_of(marked: &str) -> usize {
        let caret = marked.find('|').expect("the fixture marks the caret with `|`");
        let source = marked.replace('|', "");
        let tree = parse_java(&source).expect("parse");
        let root = tree.root_node();
        let call = enclosing_call(root, caret).expect("a call around the caret");
        let args = call.child_by_field_name("arguments").expect("arguments");
        active_argument(args, &source, caret)
    }

    #[test]
    fn the_first_argument_is_argument_zero() {
        assert_eq!(active_of("class C { void m() { f(|a, b); } }"), 0);
    }

    #[test]
    fn a_comma_moves_to_the_next_argument() {
        assert_eq!(active_of("class C { void m() { f(a, |b); } }"), 1);
        assert_eq!(active_of("class C { void m() { f(a, b, |c); } }"), 2);
    }

    /// A comma inside a nested call belongs to that call, not to this one.
    #[test]
    fn a_nested_calls_commas_do_not_count() {
        assert_eq!(active_of("class C { void m() { f(g(a, b), |c); } }"), 1);
    }

    /// Nor does one inside a string.
    #[test]
    fn a_comma_in_a_literal_does_not_count() {
        assert_eq!(active_of("class C { void m() { f(\"a,b\", |c); } }"), 1);
    }

    /// A generic argument's comma is inside `<>`, which the depth counter tracks.
    #[test]
    fn a_generic_arguments_comma_does_not_count() {
        assert_eq!(active_of("class C { void m() { f(new HashMap<String, Integer>(), |c); } }"), 1);
    }

    #[test]
    fn a_name_that_repeats_the_argument_is_not_worth_hinting() {
        assert!(!worth_naming("source", "source"));
        assert!(!worth_naming("orderSource", "source"));
        assert!(!worth_naming("SOURCE", "source"));
        assert!(worth_naming("a", "source"));
        assert!(worth_naming("500", "amount"));
    }

    #[test]
    fn a_lambda_argument_is_left_alone() {
        assert!(!worth_naming("x -> x.id()", "mapper"));
        assert!(!worth_naming("Order::id", "mapper"));
    }

    #[test]
    fn a_long_expression_is_left_alone() {
        assert!(!worth_naming(
            "repository.findById(identifier).orElseThrow()",
            "order"
        ));
    }

    #[test]
    fn parameter_spans_locate_each_rendered_parameter() {
        let params = vec![
            ("String".to_string(), "source".to_string()),
            ("long".to_string(), "amount".to_string()),
        ];
        let label = render_signature("transfer", &params, "void");
        let spans = parameter_spans(&label, &params);
        assert_eq!(&label[spans[0].0..spans[0].1], "String source");
        assert_eq!(&label[spans[1].0..spans[1].1], "long amount");
    }
}
