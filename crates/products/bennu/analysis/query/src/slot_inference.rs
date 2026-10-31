//! The type a call argument wants when the parameter is a type variable: `Collections.addAll(items, |` wants an `T`,
//! and `T` is what `items` holds; `Pair.of(NAZIONI, |` wants an `R`, and `R` is what the call around it wants of the
//! `Pair` it returns.
//!
//! Two sources, the same way the compiler uses them: the arguments already written (their types bind the variables of
//! the parameters they fill), then, for a variable still free, the type the call's own value is wanted as. Abstains
//! when a variable stays free or the overloads disagree.

use bennu_java::prelude::{
    extract_symbols, infer_node_type_cached, parse_java, seen_as, substitute, FileSymbols, InferCache, Member, TypeRef,
    TypeResolver,
};
use tree_sitter::Node;

use crate::hints::{active_argument, admits_so_far, argument_count, call_candidates, closes_at_caret, enclosing_call};

/// How many calls deep the wanted type is followed outwards.
const MAX_DEPTH: usize = 3;

/// Whether `t` is a bare type variable (`T`, `E`, `R`, `T1`), as opposed to a class: no package, one or two characters.
pub(crate) fn is_variable(t: &TypeRef) -> bool {
    t.type_args.is_empty()
        && t.dims == 0
        && (1..=2).contains(&t.binary_name.len())
        && t.binary_name.starts_with(|c: char| c.is_ascii_uppercase())
        && t.binary_name.chars().all(|c| c.is_ascii_alphanumeric())
}

fn mentions_variable(t: &TypeRef) -> bool {
    is_variable(t) || t.type_args.iter().any(mentions_variable)
}

/// Binds the variables of `param` to the matching parts of `arg` (`Collection<? super T>` against `List<Pair>` gives
/// `T = Pair`). The first binding of a variable wins; a raw or unrelated argument binds nothing.
fn unify(param: &TypeRef, arg: &TypeRef, resolver: &dyn TypeResolver, bindings: &mut Vec<(String, TypeRef)>) {
    if is_variable(param) {
        if !arg.binary_name.is_empty() && !mentions_variable(arg) && !bindings.iter().any(|(v, _)| *v == param.binary_name) {
            bindings.push((param.binary_name.clone(), TypeRef { wildcard: false, ..arg.clone() }));
        }
        return;
    }
    if param.type_args.is_empty() || param.dims != arg.dims {
        return;
    }
    let viewed = if arg.binary_name == param.binary_name {
        arg.clone()
    } else {
        match seen_as(resolver, arg, &param.binary_name) {
            Some(v) => v,
            None => return,
        }
    };
    if viewed.type_args.len() != param.type_args.len() {
        return;
    }
    for (p, a) in param.type_args.iter().zip(&viewed.type_args) {
        unify(p, a, resolver, bindings);
    }
}

/// The parameter an argument at `index` fills: the last parameter, element-wise, for a varargs tail.
fn param_at(m: &Member, index: usize) -> Option<TypeRef> {
    let last = m.params.len().checked_sub(1)?;
    let p = m.params.get(index.min(last))?;
    if index >= last && p.dims > 0 {
        return Some(TypeRef { dims: p.dims - 1, ..p.clone() });
    }
    m.params.get(index).cloned()
}

fn apply(ty: &TypeRef, bindings: &[(String, TypeRef)]) -> TypeRef {
    let (names, args): (Vec<String>, Vec<TypeRef>) = bindings.iter().cloned().unzip();
    substitute(ty, &names, &args)
}

/// The type the argument at `offset` takes, with the type variables of its parameter settled; `source` must parse
/// with an argument at the caret.
pub fn inferred_slot_type(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<TypeRef> {
    inferred_slot_type_with(source, offset, resolver, closes_at_caret(source, offset))
}

/// [`inferred_slot_type`] for a buffer whose closing parenthesis was added by the caller, which says whether the
/// user's own text closes the call.
pub fn inferred_slot_type_with(source: &str, offset: usize, resolver: &dyn TypeResolver, closed: bool) -> Option<TypeRef> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let repaired = format!("{}x{}", &source[..offset], &source[offset..]);
    let tree = parse_java(&repaired)?;
    let root = tree.root_node();
    let symbols = extract_symbols(&repaired);
    let cache = InferCache::new();
    slot_in(&root, &repaired, &symbols, offset, resolver, &cache, 0, closed)
}

/// What a call argument's declared parameter says about the text that may fill it.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericSlot {
    /// The parameter is a concrete type (or the call is not one we can read): the declaration decides, not this.
    Fixed,
    /// The parameter is a type variable and the surrounding call settles it to this type.
    Settled(TypeRef),
    /// The parameter is a type variable nothing settles: any text is a guess about its type.
    Free,
}

/// Whether the argument at `offset` fills a parameter that is a type variable (`<T> Matcher<T> helper(T value)`), and
/// the type the calls around it make of that variable.
pub fn generic_slot(source: &str, offset: usize, resolver: &dyn TypeResolver) -> GenericSlot {
    if !source.is_char_boundary(offset) {
        return GenericSlot::Fixed;
    }
    let repaired = format!("{}x{}", &source[..offset], &source[offset..]);
    let Some(tree) = parse_java(&repaired) else { return GenericSlot::Fixed };
    let root = tree.root_node();
    let symbols = extract_symbols(&repaired);
    let cache = InferCache::new();
    let closed = closes_at_caret(source, offset);
    match slot_core(&root, &repaired, &symbols, offset, resolver, &cache, 0, closed) {
        Some((true, Some(t))) => GenericSlot::Settled(t),
        Some((true, None)) => GenericSlot::Free,
        _ => GenericSlot::Fixed,
    }
}

/// The static type of `text` written as the argument that starts at `start` and runs to the caret at `offset`;
/// `None` when it cannot be typed (a name that is not there, a literal).
pub fn written_type(source: &str, start: usize, offset: usize, text: &str, resolver: &dyn TypeResolver) -> Option<TypeRef> {
    if start > offset || !source.is_char_boundary(start) || !source.is_char_boundary(offset) {
        return None;
    }
    let buf = format!("{}{}){}", &source[..start], text, &source[offset..]);
    bennu_java::prelude::infer_expression_type(&buf, start, start + text.len(), resolver)
}

/// Whether a value of type `value` can be passed where `wanted` is expected: the same class, or one that has it as a
/// supertype. A primitive or an array on either side is not judged here.
pub fn fits_type(value: &TypeRef, wanted: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    if value.dims != wanted.dims || value.binary_name == wanted.binary_name || wanted.binary_name == "java/lang/Object" {
        return true;
    }
    !value.binary_name.contains('/') || seen_as(resolver, value, &wanted.binary_name).is_some()
}

fn slot_in(root: &Node, source: &str, symbols: &FileSymbols, offset: usize, resolver: &dyn TypeResolver, cache: &InferCache, depth: usize, closed: bool) -> Option<TypeRef> {
    slot_core(root, source, symbols, offset, resolver, cache, depth, closed)?.1
}

/// [`slot_in`] that also says whether the parameter was a type variable in some overload.
fn slot_core(root: &Node, source: &str, symbols: &FileSymbols, offset: usize, resolver: &dyn TypeResolver, cache: &InferCache, depth: usize, closed: bool) -> Option<(bool, Option<TypeRef>)> {
    if depth > MAX_DEPTH {
        return None;
    }
    let call = enclosing_call(*root, offset)?;
    let args = call.child_by_field_name("arguments")?;
    let (_, candidates) = call_candidates(&call, root, source, symbols, resolver, cache)?;
    let active = active_argument(args, source, offset);
    let mut cursor = args.walk();
    let written: Vec<Node> = args.named_children(&mut cursor).filter(|c| !matches!(c.kind(), "line_comment" | "block_comment")).collect();
    let count = argument_count(args);
    let wanted = count.max(active + 1);
    let may_grow = !closed && active + 1 >= count;
    let admitting: Vec<&Member> = candidates.iter().filter(|m| admits_so_far(m, wanted, may_grow)).collect();
    let pool: Vec<&Member> = if admitting.is_empty() { candidates.iter().filter(|m| m.params.len() > active).collect() } else { admitting };
    let mut found: Vec<TypeRef> = Vec::new();
    let mut generic = false;
    for m in pool {
        let slot = param_at(m, active)?;
        generic |= mentions_variable(&slot);
        let mut bindings: Vec<(String, TypeRef)> = Vec::new();
        for (i, arg) in written.iter().enumerate().take(active) {
            if let (Some(p), Some(a)) = (param_at(m, i), infer_node_type_cached(root, source, symbols, arg, resolver, cache)) {
                unify(&p, &a, resolver, &mut bindings);
            }
        }
        let mut settled = apply(&slot, &bindings);
        if mentions_variable(&settled) {
            // what the call's own value is wanted as binds the rest: `Pair.of(a, |)` inside a slot that wants `Pair<A, List<B>>`
            if let Some(outer) = wanted_value(&call, root, source, symbols, resolver, cache, depth) {
                unify(&m.return_type, &outer, resolver, &mut bindings);
                settled = apply(&slot, &bindings);
            }
        }
        if mentions_variable(&settled) {
            return Some((generic, None));
        }
        if !found.iter().any(|f| f.binary_name == settled.binary_name && f.dims == settled.dims) {
            found.push(settled);
        }
    }
    match found.as_slice() {
        [one] => Some((generic, Some(one.clone()))),
        _ => Some((generic, None)),
    }
}

/// The type `call`'s value is wanted as: the slot of the call around it, or the declaration or assignment it sits in.
fn wanted_value(call: &Node, root: &Node, source: &str, symbols: &FileSymbols, resolver: &dyn TypeResolver, cache: &InferCache, depth: usize) -> Option<TypeRef> {
    let at = call.start_byte();
    if call.parent().is_some_and(|p| p.kind() == "argument_list") {
        return slot_in(root, source, symbols, at, resolver, cache, depth + 1, false);
    }
    bennu_java::prelude::expected_type(source, at, resolver)
}
