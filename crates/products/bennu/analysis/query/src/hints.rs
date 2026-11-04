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
    bound_overload, infer_node_type_cached, extract_symbols, method_admits_argc, parse_java,
    ClassMembers, FileSymbols, InferCache, Member, MemberKind, TypeRef, TypeResolver,
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
    /// What the hint says when the pointer rests on it — the parameter's declared type, for a
    /// parameter-name hint. Empty when there is nothing more to say than the label already does.
    ///
    /// The name answers *which argument is this*; the type answers *what does it want*, which is
    /// the next question and the one the reader currently has to open the declaration for. It is a
    /// tooltip rather than more label text because the label is drawn inside the line: a hint that
    /// grew to `method: String` would push the code around to say something most reads do not
    /// need.
    pub tooltip: String,
}

/// Where a **library** method's parameter names come from, when they can be known at all.
///
/// A class file does not carry them: `-parameters` is off by default and almost no published jar
/// turns it on, so the member model has types and nothing else — which is why a call into a library
/// gets no name hints while a call into the project gets them from its own source.
///
/// The library's **source** does carry them, and Bennu already fetches it: the `-sources.jar` behind
/// "Download sources", and the JDK's own `src.zip`. So the rule is exactly that — **names appear
/// when the source is there, and never otherwise.** A decompiled stub is not an answer: its
/// `arg0`, `arg1` are placeholders, and drawing one as if it were the parameter's name would be the
/// one thing a hint must never do, which is state something the code does not say.
///
/// Implemented in the `be` layer, which owns the source archives; `None` from any of it means "no
/// hint", never "guess".
pub trait LibraryParamNames {
    /// The names `method` declares, in order, for the overload taking `arity` parameters. `None`
    /// when the type has no downloaded source, the method is not found in it, or the overload is
    /// ambiguous.
    fn names_for(&self, owner_binary: &str, method: &str, arity: usize) -> Option<Vec<String>>;
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
    let (_owner, candidates) = call_candidates(&call, &root, source, &symbols, resolver, &cache)?;

    let argc = argument_count(args);
    let active = active_argument(args, source, offset);

    // Prefer an overload whose arity admits where the caret is: with `2` typed you mean the 3-arg
    // one even though `2` alone also fits the 2-arg one.
    let wanted = argc.max(active + 1);
    let admitting: Vec<&Member> = candidates
        .iter()
        .filter(|m| method_admits_argc(m, wanted))
        .collect();
    let shown: Vec<&Member> = if admitting.is_empty() { candidates.iter().collect() } else { admitting };
    // The overload the written arguments bind to (the shared applicability rules), when they settle
    // it and it still fits where the caret is; otherwise the first that does.
    let bound = bound_overload(&root, source, &symbols, &call, &candidates, resolver, &cache)
        .filter(|m| shown.iter().any(|s| std::ptr::eq(*s, *m)));
    let picked = match bound {
        Some(m) => m,
        None => *shown.first()?,
    };
    let picked_index = shown.iter().position(|s| std::ptr::eq(*s, picked)).unwrap_or(0);

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
        overload: (candidates.len() > 1).then_some((picked_index, shown.len())),
    })
}

/// What the argument at `offset` is passed as: its type, and the parameter's name when the callee carries one.
#[derive(Debug, Clone)]
pub(crate) struct ArgumentSlot {
    pub(crate) ty: TypeRef,
    pub(crate) name: Option<String>,
}

/// Whether the call the caret is in is surely closed right here: its `)` follows the caret on the same line.
///
/// A `)` further down is as likely the closer of the call AROUND this one — `Collections.addAll(a,⏎ Pair.of(|⏎ )` —
/// and then the arguments written so far are only a start.
pub(crate) fn closes_at_caret(source: &str, offset: usize) -> bool {
    source.get(offset..).is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with(')'))
}

/// Whether `m` can still be the overload of a call with `wanted` arguments written so far. When more may follow (the
/// caret is in the last argument and the call is not surely closed) an overload with MORE parameters is still in the
/// running: `Pair.of(|` is `of(L, R)` as much as `of(Entry<L, R>)`.
pub(crate) fn admits_so_far(m: &Member, wanted: usize, may_grow: bool) -> bool {
    method_admits_argc(m, wanted) || (may_grow && m.params.len() >= wanted)
}

/// Every overload that admits the position of the argument at `offset`, as the slot it gives that argument. `closed` is
/// whether the call is surely closed at the caret: see [`closes_at_caret`].
/// How many distinct questions the slot cache remembers. One completion asks one (source, caret) pair; a second
/// covers the ghost-text request that follows it.
const SLOT_CACHE_SIZE: usize = 4;

/// How long a remembered answer is trusted. Short on purpose: the answer depends on the index, which the cache
/// cannot see change — a second is long enough to cover the half-dozen passes of ONE completion request and too
/// short for anyone to notice a stale type after an edit elsewhere.
const SLOT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(1);

struct SlotEntry {
    fingerprint: u64,
    len: usize,
    offset: usize,
    closed: bool,
    /// The resolver the answer was worked out against, by address: a rebuilt index is a new resolver.
    resolver: usize,
    at: std::time::Instant,
    slots: Vec<ArgumentSlot>,
}

thread_local! {
    static SLOT_CACHE: std::cell::RefCell<Vec<SlotEntry>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn slot_fingerprint(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// [`compute_slot_candidates`], remembered per (buffer, caret, resolver) for [`SLOT_CACHE_TTL`].
///
/// Completing inside a call's parentheses asks "what does this argument take?" from half a dozen passes — the
/// type names, the enum constants, the functional types, the callee's statics, the locals that fit, the `Class<T>`
/// slot — and each used to parse the buffer and type the callee's receiver chain again for the identical answer.
/// Inference is the expensive part of a keystroke here, so it is done once.
fn slot_candidates(source: &str, offset: usize, resolver: &dyn TypeResolver, closed: bool) -> Vec<ArgumentSlot> {
    // Tests build many resolvers at the same stack address over the same text; a remembered answer from the
    // previous one would be a wrong answer for the next.
    if cfg!(test) {
        return compute_slot_candidates(source, offset, resolver, closed);
    }
    let fingerprint = slot_fingerprint(source);
    let who =(resolver as *const dyn TypeResolver).cast::<()>() as usize;
    let hit = SLOT_CACHE.with(|cell| {
        cell.borrow()
            .iter()
            .find(|e| {
                e.fingerprint == fingerprint
                    && e.len == source.len()
                    && e.offset == offset
                    && e.closed == closed
                    && e.resolver == who
                    && e.at.elapsed() < SLOT_CACHE_TTL
            })
            .map(|e| e.slots.clone())
    });
    if let Some(slots) = hit {
        return slots;
    }
    let slots = compute_slot_candidates(source, offset, resolver, closed);
    SLOT_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.insert(
            0,
            SlotEntry {
                fingerprint,
                len: source.len(),
                offset,
                closed,
                resolver: who,
                at: std::time::Instant::now(),
                slots: slots.clone(),
            },
        );
        cache.truncate(SLOT_CACHE_SIZE);
    });
    slots
}

fn compute_slot_candidates(source: &str, offset: usize, resolver: &dyn TypeResolver, closed: bool) -> Vec<ArgumentSlot> {
    // `f(a, |)` does not parse as an argument list, and the error recovery counts the arguments wrongly: a
    // placeholder at the caret makes it one. The buffer as written is the fallback.
    if source.is_char_boundary(offset) {
        let repaired = format!("{}x{}", &source[..offset], &source[offset..]);
        let slots = slot_candidates_in(&repaired, offset, resolver, closed);
        if !slots.is_empty() {
            return slots;
        }
    }
    slot_candidates_in(source, offset, resolver, closed)
}

fn slot_candidates_in(source: &str, offset: usize, resolver: &dyn TypeResolver, closed: bool) -> Vec<ArgumentSlot> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let symbols = extract_symbols(source);
    let cache = InferCache::new();
    let Some(call) = enclosing_call(root, offset) else { return Vec::new() };
    let Some(args) = call.child_by_field_name("arguments") else { return Vec::new() };
    let Some((_, candidates)) = call_candidates(&call, &root, source, &symbols, resolver, &cache) else { return Vec::new() };
    let active = active_argument(args, source, offset);
    let count = argument_count(args);
    let wanted = count.max(active + 1);
    let may_grow = !closed && active + 1 >= count;
    // The overloads whose arity admits the call as written so far; with the call half-written (`transfer(|`) none
    // does, and every overload that has a parameter at this position is still in the running.
    let admitting: Vec<&Member> = candidates.iter().filter(|m| admits_so_far(m, wanted, may_grow)).collect();
    let pool: Vec<&Member> = if admitting.is_empty() { candidates.iter().filter(|m| m.params.len() > active).collect() } else { admitting };
    pool.into_iter()
        .filter_map(|m| {
            let ty = m.params.get(active)?.clone();
            let name = named_parameters(m).into_iter().nth(active).and_then(|(_, n)| n);
            Some(ArgumentSlot { ty, name })
        })
        .collect()
}

/// The class a `Class<T>` argument must be a subtype of, when the callee bounds its type parameter (`<T extends ParserXml<X, Z>>
/// … Class<T> parse_xml`): what is passed there is `Something.class` for a `Something` that implements the bound. `None` when the slot
/// is not a `Class<T>` of every overload that admits it, or the type parameter has no bound the index can read.
pub fn class_slot_bound(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<TypeRef> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let repaired = format!("{}x{}", &source[..offset], &source[offset..]);
    let tree = parse_java(&repaired)?;
    let root = tree.root_node();
    let symbols = extract_symbols(&repaired);
    let cache = InferCache::new();
    let call = enclosing_call(root, offset)?;
    let args = call.child_by_field_name("arguments")?;
    let (_, candidates) = call_candidates(&call, &root, &repaired, &symbols, resolver, &cache)?;
    let active = active_argument(args, &repaired, offset);
    let mut bound: Option<String> = None;
    for m in candidates.iter().filter(|m| m.params.len() > active) {
        let slot = &m.params[active];
        if slot.binary_name != "java/lang/Class" {
            return None;
        }
        let text = m.method_type_param_bound(&slot.type_args.first()?.binary_name)?;
        let simple = text.split('<').next()?.trim().to_string();
        if bound.as_ref().is_some_and(|b| *b != simple) {
            return None;
        }
        bound = Some(simple);
    }
    let simple = bound?;
    let binary = resolver.resolve_simple_name(&simple, &symbols.imports)?;
    Some(TypeRef::simple(binary))
}

/// One overload of the callee, as an argument list: the type of each parameter and its name when the callee carries it.
#[derive(Debug, Clone)]
pub(crate) struct Overload {
    pub(crate) params: Vec<(TypeRef, Option<String>)>,
}

/// The call whose parentheses the caret has just opened (`transfer(|`): every overload it could bind to, and whether
/// the call is the whole of a statement (so that `);` closes it) or sits inside a bigger expression (`)` only).
#[derive(Debug, Clone)]
pub(crate) struct CallSite {
    pub(crate) overloads: Vec<Overload>,
    pub(crate) closes_statement: bool,
}

/// The call at `offset`, read off the buffer with its argument list made well-formed (`x);` after the caret).
pub(crate) fn call_site(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<CallSite> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let repaired = format!("{}x);{}", &source[..offset], &source[offset..]);
    let tree = parse_java(&repaired)?;
    let root = tree.root_node();
    let symbols = extract_symbols(&repaired);
    let cache = InferCache::new();
    let call = enclosing_call(root, offset)?;
    let (_, candidates) = call_candidates(&call, &root, &repaired, &symbols, resolver, &cache)?;
    let overloads = candidates
        .iter()
        .map(|m| Overload {
            params: m.params.iter().cloned().zip(named_parameters(m).into_iter().map(|(_, n)| n)).collect(),
        })
        .collect();
    Some(CallSite { overloads, closes_statement: closes_a_statement(&call) })
}

/// Whether `call` is the entire expression of a statement: an expression statement, a declaration's initialiser, a
/// `return`, or the right side of an assignment that is itself a statement. In any other place — an argument of another
/// call, an operand, a condition — the line goes on after it and a `;` would be wrong.
fn closes_a_statement(call: &Node) -> bool {
    let Some(parent) = call.parent() else { return false };
    match parent.kind() {
        "expression_statement" | "return_statement" => true,
        "variable_declarator" => parent.child_by_field_name("value").is_some_and(|v| v.id() == call.id()),
        "assignment_expression" => {
            parent.child_by_field_name("right").is_some_and(|v| v.id() == call.id())
                && parent.parent().is_some_and(|p| p.kind() == "expression_statement")
        }
        _ => false,
    }
}

/// The distinct types the argument at `offset` can take, among the overloads that admit its position — one type when
/// the callee is not overloaded there (a builder's `type(…)`), several when overloads disagree. Empty outside a call
/// or when the callee does not resolve.
pub(crate) fn argument_slot_types(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Vec<TypeRef> {
    argument_slot_types_with(source, offset, resolver, closes_at_caret(source, offset))
}

/// [`argument_slot_types`] for a buffer whose closing parenthesis was added by the caller, which says whether the
/// user's own text closes the call.
pub(crate) fn argument_slot_types_with(source: &str, offset: usize, resolver: &dyn TypeResolver, closed: bool) -> Vec<TypeRef> {
    let mut out: Vec<TypeRef> = Vec::new();
    for slot in slot_candidates(source, offset, resolver, closed) {
        if !out.iter().any(|o| o.binary_name == slot.ty.binary_name && o.dims == slot.ty.dims) {
            out.push(slot.ty);
        }
    }
    out
}

/// The one class the argument at `offset` takes when every overload that admits its position agrees — what ranks the
/// type names in `list(C|)`: the parameter's own class leads, ahead of a name that merely starts alike. `None` for a
/// primitive, an array, `Object` (which says nothing) and a slot the overloads disagree on.
pub fn argument_expected_type(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<TypeRef> {
    match argument_slot_types(source, offset, resolver).as_slice() {
        [t] if t.dims == 0 && t.binary_name != "java/lang/Object" && t.binary_name.contains('/') => Some(t.clone()),
        _ => None,
    }
}

/// The slot the argument at `offset` fills, when every overload that admits the position agrees on its type —
/// with the first parameter name any of them carries. `None` when they disagree, or outside a resolvable call.
pub(crate) fn argument_slot(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<ArgumentSlot> {
    let slots = slot_candidates(source, offset, resolver, closes_at_caret(source, offset));
    let first = slots.first()?;
    if slots.iter().any(|s| s.ty.binary_name != first.ty.binary_name || s.ty.dims != first.ty.dims) {
        return None;
    }
    Some(ArgumentSlot { ty: first.ty.clone(), name: slots.iter().find_map(|s| s.name.clone()) })
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
pub(crate) fn enclosing_call<'t>(root: Node<'t>, offset: usize) -> Option<Node<'t>> {
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
pub(crate) fn call_candidates(
    call: &Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<(String, Vec<Member>)> {
    let bytes = source.as_bytes();
    // `new Foo(…)` binds to a constructor, which the member model keeps under `<init>`.
    if call.kind() == "object_creation_expression" {
        // The creation expression itself: it is typed (generic arguments included), where the bare type node is not an
        // expression and used to come back unresolved, which left every constructor argument slot unanswered.
        let resolved = infer_node_type_cached(root, source, symbols, call, resolver, cache)?;
        let cm = resolver.members_of(&resolved.binary_name)?;
        // `new Box<Order>(…)` takes an `Order`, not a `T` — the arguments are written right there.
        let mut ctors: Vec<Member> = cm
            .methods
            .iter()
            .filter(|m| m.name == "<init>")
            .map(|m| substituted(m, &cm.type_params, &resolved.type_args))
            .collect();
        if ctors.is_empty() && has_implicit_constructor(&cm) {
            ctors.push(Member::method("<init>", TypeRef::simple("void"), Vec::new()));
        }
        let ctors = non_empty(ctors)?;
        return Some((resolved.binary_name, ctors));
    }
    let name_node = call.child_by_field_name("name")?;
    let name = name_node.utf8_text(bytes).ok()?;

    // An unqualified call is on the enclosing type; a qualified one on whatever the receiver is.
    let owner = match call.child_by_field_name("object") {
        // a value, or else a class name: `Collections.addAll(…)` is a static call on the type
        Some(recv) => infer_node_type_cached(root, source, symbols, &recv, resolver, cache)
            .or_else(|| bennu_java::prelude::type_receiver_cached(root, source, symbols, &recv, resolver, cache))?,
        None => {
            let binary = bennu_java::prelude::enclosing_type_binary(source, call.start_byte())?;
            bennu_java::prelude::TypeRef::simple(binary)
        }
    };
    let mut out: Vec<Member> = Vec::new();
    collect_named(resolver, &owner, name, &mut out);
    // The DECLARING type, not the receiver: an inherited method's parameter names are written in
    // the file that declares it, and asking the subclass's source for them finds nothing.
    let declared_on = bennu_java::prelude::declaring_method(resolver, &owner, name)
        .map(|t| t.binary_name)
        .unwrap_or(owner.binary_name);
    non_empty(out).map(|ms| (declared_on, ms))
}

/// Whether `new Foo(` has a constructor nobody wrote: the no-argument one the compiler adds to a concrete class that
/// declares none. Not for an interface, an abstract class, an enum or a record (its canonical one is modelled), and
/// not for a type with members the index cannot see — a generated constructor may be among them.
fn has_implicit_constructor(cm: &ClassMembers) -> bool {
    let f = &cm.flags;
    !(f.is_interface || f.is_abstract || f.is_enum || f.is_annotation || f.is_record || f.has_hidden_members)
}

fn non_empty(v: Vec<Member>) -> Option<Vec<Member>> {
    (!v.is_empty()).then_some(v)
}

/// Gather every method called `name` on `owner` and its supertypes, most-derived first — **with the
/// receiver's type arguments substituted in**.
///
/// The substitution is the difference between a strip that describes the call and one that
/// describes the declaration. `Optional<PathPattern>.orElseThrow(…)` is declared
/// `T orElseThrow(Supplier<? extends X>)`, and a strip reading `: T` answers a question nobody
/// asked: the reader knows the method, they are looking to be told what it gives *here*.
///
/// It is exact rather than a guess, and free, because the hierarchy walk already carries it: each
/// [`Ancestor`](bennu_java::prelude::Ancestor) arrives as seen from where the walk started
/// (`List<String>`, not `List<E>`), so a method inherited from a generic supertype substitutes
/// against that supertype's own parameter list. A variable nothing binds — a method-level `<X>`,
/// whose argument comes from the call rather than the receiver — is left as written, which is the
/// honest answer and what it already showed.
fn collect_named(
    resolver: &dyn TypeResolver,
    owner: &bennu_java::prelude::TypeRef,
    name: &str,
    out: &mut Vec<Member>,
) {
    bennu_java::prelude::walk_up::<()>(resolver, owner, |a| {
        for m in &a.members.methods {
            if m.kind == MemberKind::Method && m.name == name {
                let m = substituted(m, &a.members.type_params, &a.ty.type_args);
                // An override arrives again from every level it is declared at; the first (most
                // derived) is the one the call binds to, and the shared walk is breadth-first, so
                // that is the one that gets here first.
                if !out.iter().any(|k| k.params == m.params) {
                    out.push(m);
                }
            }
        }
        None
    });
}

/// `m` with each type variable of `params` replaced by the matching entry of `args`, in its
/// parameters and its return type. See [`collect_named`].
fn substituted(m: &Member, params: &[String], args: &[bennu_java::prelude::TypeRef]) -> Member {
    if params.is_empty() || args.is_empty() {
        return m.clone();
    }
    let sub = bennu_java::prelude::substitute;
    Member {
        return_type: sub(&m.return_type, params, args),
        params: m.params.iter().map(|p| sub(p, params, args)).collect(),
        ..m.clone()
    }
}

/// How many arguments the list holds — 0 for an empty one.
pub(crate) fn argument_count(args: Node) -> usize {
    let mut c = args.walk();
    args.named_children(&mut c).filter(|n| !is_trivia(*n)).count()
}

/// Which argument `offset` is on: the number of top-level commas before it.
///
/// Counted over the source text rather than the tree because a list being typed is a broken tree —
/// `f(a, ` has no second argument node for the caret to be inside of, and that is exactly the
/// moment the strip has to move the mark along.
pub(crate) fn active_argument(args: Node, source: &str, offset: usize) -> usize {
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
    inlay_hints_with(source, resolver, None)
}

/// [`inlay_hints`], plus a source of **library** parameter names — see [`LibraryParamNames`].
///
/// Separate entry point rather than a changed signature: every test and harness in the workspace
/// asks the plain question, and only the `be` layer is in a position to answer the other one.
pub fn inlay_hints_with(
    source: &str,
    resolver: &dyn TypeResolver,
    lib: Option<&dyn LibraryParamNames>,
) -> Vec<InlayHint> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let symbols = extract_symbols(source);
    let cache = InferCache::new();
    let mut out = Vec::new();
    walk_hints(root, &root, source, &symbols, resolver, &cache, lib, &mut out);
    out.sort_by_key(|h| h.offset);
    out
}

#[allow(clippy::too_many_arguments)]
fn walk_hints(
    node: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    lib: Option<&dyn LibraryParamNames>,
    out: &mut Vec<InlayHint>,
) {
    match node.kind() {
        "method_invocation" | "object_creation_expression" => {
            parameter_name_hints(&node, root, source, symbols, resolver, cache, lib, out);
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
        walk_hints(child, root, source, symbols, resolver, cache, lib, out);
    }
}

/// `transfer(source: from, target: to, amount: 500)` — a name in front of each argument that does
/// not already say what it is.
#[allow(clippy::too_many_arguments)]
fn parameter_name_hints(
    call: &Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    lib: Option<&dyn LibraryParamNames>,
    out: &mut Vec<InlayHint>,
) {
    let Some(args) = call.child_by_field_name("arguments") else { return };
    let bytes = source.as_bytes();
    let mut c = args.walk();
    let arg_nodes: Vec<Node> = args.named_children(&mut c).filter(|n| !is_trivia(*n)).collect();
    if arg_nodes.is_empty() {
        return;
    }
    let Some((owner, candidates)) = call_candidates(call, root, source, symbols, resolver, cache)
    else {
        return;
    };
    // Only an unambiguous binding earns a hint. With two overloads still standing, the names could
    // come from either, and a name from the wrong one is a lie about the code.
    //
    // Which overload it is comes from the one set of applicability rules go-to, hover and inference
    // use (`bennu_java`'s `bound_overload`): arity, then the arguments — a lambda fits only a
    // functional interface of its parameter count, `null` any reference, a typed value by boxing,
    // widening and subtyping, and an argument nothing could type decides nothing. Arity alone lost
    // the names of `addAllowedMethod("*")` beside `(HttpMethod)`, and of `uri("/x", b -> …)` beside
    // `uri(String, Map)`.
    let Some(picked) = bound_overload(root, source, symbols, call, &candidates, resolver, cache)
    else {
        return;
    };
    let params = named_parameters(picked);
    if params.len() != arg_nodes.len() {
        return; // a varargs call, whose tail has no one name
    }
    // A class file carries no parameter names unless it was compiled with `-parameters`, so a call
    // into a library has none — but its **source** does, when it has been downloaded. Asked only
    // when the member model has nothing, and only ever answered from real source: see
    // [`LibraryParamNames`].
    let from_source = params
        .iter()
        .all(|(_, n)| n.is_none())
        .then(|| lib?.names_for(&owner, &picked.name, arg_nodes.len()))
        .flatten()
        .filter(|names| names.len() == arg_nodes.len());
    for (i, (arg, (_, name))) in arg_nodes.iter().zip(params.iter()).enumerate() {
        // No name, no hint. The placeholder the override generator uses — `arg0` — would read here
        // as a claim that the parameter is *called* that. Saying nothing is the true answer.
        let owned;
        let name = match (name, &from_source) {
            (Some(n), _) => n,
            (None, Some(names)) => {
                owned = names[i].clone();
                &owned
            }
            (None, None) => continue,
        };
        let Ok(text) = arg.utf8_text(bytes) else { continue };
        if !worth_naming(text, name) {
            continue;
        }
        out.push(InlayHint {
            offset: arg.start_byte(),
            label: format!("{name}:"),
            before: true,
            tooltip: picked
                .params
                .get(i)
                .map(|p| format!("{name}: {}", render_type(p)))
                .unwrap_or_default(),
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
            // The label IS the type. A tooltip repeating it would be a hover that says nothing.
            tooltip: String::new(),
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
            // The label IS the type.
            tooltip: String::new(),
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

/// Where the caret is in a call's argument list: the callee's simple name (`filter`; `new Foo` for a constructor), the
/// index of the argument the caret is in, and the byte offset where that argument starts. The call may be unfinished
/// (`list.filter(Boo|`): the buffer is repaired the ways an editor leaves it — a missing `)`, a missing `);`.
/// `None` outside a call's parentheses. Built for the argument habits, which need nothing of the callee but its name.
pub fn call_site_name_and_index(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    let mut offset = offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let (head, tail) = source.split_at(offset);
    // as written, then with the closers an unfinished call lacks; the placeholder keeps `f(a, |)` an argument list
    for repaired in [source.to_string(), format!("{head}x){tail}"), format!("{head}x);{tail}"), format!("{head}x));{tail}")] {
        let Some(tree) = parse_java(&repaired) else { continue };
        let root = tree.root_node();
        let Some(call) = enclosing_call(root, offset) else { continue };
        let Some(args) = call.child_by_field_name("arguments") else { continue };
        let name = match call.kind() {
            "method_invocation" => call.child_by_field_name("name").and_then(|n| n.utf8_text(repaired.as_bytes()).ok()).map(str::to_string),
            _ => call.child_by_field_name("type").and_then(|t| t.utf8_text(repaired.as_bytes()).ok()).map(|t| format!("new {}", t.split('<').next().unwrap_or(t).trim())),
        };
        let Some(name) = name else { continue };
        // the argument starts after the last top-level comma (or the `(`) before the caret
        let from = args.start_byte() + 1;
        let (mut depth, mut start) = (0i32, from);
        let mut chars = repaired[from..offset.max(from)].char_indices();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '"' | '\'' => {
                    let mut escaped = false;
                    for (_, c) in chars.by_ref() {
                        if escaped {
                            escaped = false;
                        } else if c == '\\' {
                            escaped = true;
                        } else if c == ch {
                            break;
                        }
                    }
                }
                ',' if depth == 0 => start = from + i + 1,
                _ => {}
            }
        }
        return Some((name, active_argument(args, &repaired, offset), start));
    }
    None
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

    /// The strip is about the call, and `Optional<PathPattern>.orElseThrow(…)` gives a
    /// `PathPattern`. Answering `T` describes the declaration, which the reader can already see.
    #[test]
    fn the_receivers_type_arguments_reach_the_rendered_signature() {
        use bennu_java::prelude::TypeRef;
        let m = Member::method(
            "orElseThrow",
            TypeRef::simple("T"),
            vec![TypeRef::simple("java/util/function/Supplier")],
        );
        let bound = substituted(&m, &["T".to_string()], &[TypeRef::simple("com/acme/PathPattern")]);
        assert_eq!(bound.return_type.binary_name, "com/acme/PathPattern");
        assert_eq!(render_type(&bound.return_type), "PathPattern");
    }

    /// A method-level variable — `<X>` on `orElseThrow`, bound by the argument rather than by the
    /// receiver — is not in the class's list and must be left exactly as written.
    #[test]
    fn a_variable_the_receiver_does_not_bind_is_left_alone() {
        use bennu_java::prelude::TypeRef;
        let m = Member::method("orElseThrow", TypeRef::simple("X"), vec![]);
        let bound = substituted(&m, &["T".to_string()], &[TypeRef::simple("com/acme/PathPattern")]);
        assert_eq!(bound.return_type.binary_name, "X");
    }

    /// A raw receiver has nothing to substitute with, and inventing `Object` would be a claim.
    #[test]
    fn a_raw_receiver_changes_nothing() {
        use bennu_java::prelude::TypeRef;
        let m = Member::method("get", TypeRef::simple("E"), vec![]);
        assert_eq!(substituted(&m, &["E".to_string()], &[]).return_type.binary_name, "E");
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
