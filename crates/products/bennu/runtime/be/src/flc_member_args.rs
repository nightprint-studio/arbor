//! The usual call on a variable, where the variable is whichever one is at hand.
//!
//! `new ByteArrayInputStream(bao.toByteArray())`, `new ByteArrayInputStream(out.toByteArray())`, `…(buffer.toByteArray())`:
//! the argument is a different text in every file, so no single text is a habit — but its *shape* is: `.toByteArray()` called
//! on a variable. This counts that shape per call place, and when a place has one it is drawn with the variable in scope
//! whose type has that method, the nearest one declared. It is the layer under `flc_habits`, which counts whole texts.
//!
//! Needs enough sightings to speak, like the others; a place with a dozen of them is the point of it.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_java::prelude::{extract_symbols, parse_java, visible_bindings, walk_up, TypeRef, TypeResolver};
use tree_sitter::Node;

use crate::flc_habits::Candidate;
use crate::index_service::IndexService;

const MIN_SIGHTINGS: u32 = 5;
const MIN_SHARE: f32 = 0.6;
/// Longest argument worth counting.
const MAX_LEN: usize = 60;
/// Distinct (place, shape) pairs kept.
const MAX_ENTRIES: usize = 20_000;

type Place = (String, usize);

/// What to draw after the caret, and how sure that is.
pub(crate) struct MemberArg {
    pub text: String,
    pub confidence: f32,
}

#[derive(Default)]
struct Held {
    by_file: HashMap<String, Vec<(Place, String)>>,
    table: HashMap<Place, HashMap<String, u32>>,
    entries: usize,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn add_file(file: &str, source: &str) {
    let found = scan(source);
    let mut h = lock();
    remove_locked(&mut h, file);
    let mut kept = Vec::with_capacity(found.len());
    for (place, shape) in found {
        let known = h.table.get(&place).is_some_and(|m| m.contains_key(&shape));
        if !known && h.entries >= MAX_ENTRIES {
            continue;
        }
        let count = h.table.entry(place.clone()).or_default().entry(shape.clone()).or_insert(0);
        let fresh = *count == 0;
        *count += 1;
        if fresh {
            h.entries += 1;
        }
        kept.push((place, shape));
    }
    h.by_file.insert(file.to_string(), kept);
}

/// The calls on a variable counted per place, for the page that shows them.
pub(crate) fn report() -> Vec<(String, Vec<(String, u32)>)> {
    let h = lock();
    let mut out: Vec<(String, Vec<(String, u32)>)> = h
        .table
        .iter()
        .map(|(place, row)| {
            let mut rows: Vec<(String, u32)> = row.iter().map(|(t, n)| (t.clone(), *n)).collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            (format!("{}#{}", place.0, place.1), rows)
        })
        .collect();
    out.sort_by_key(|(key, rows)| (std::cmp::Reverse(rows.iter().map(|r| r.1).sum::<u32>()), key.clone()));
    out
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, two_level, Sizes};
    let h = lock();
    let place = |p: &Place| text(&p.0) + 8;
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.iter().map(|(p, t)| place(p) + text(t)).sum());
    }
    s.shared = two_level(&h.table, place);
    s.shared_entries = h.entries;
    s
}

/// Forgets everything counted, for *Forget what Bennu learned*.
pub(crate) fn clear() {
    *lock() = Held::default();
}

/// Files read, and (place, shape) pairs counted, for the memory breakdown.
pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.entries)
}

pub(crate) fn remove_file(file: &str) {
    remove_locked(&mut lock(), file);
}

fn remove_locked(h: &mut Held, file: &str) {
    for (place, shape) in h.by_file.remove(file).unwrap_or_default() {
        let Some(row) = h.table.get_mut(&place) else { continue };
        if let Some(n) = row.get_mut(&shape) {
            *n -= 1;
            if *n == 0 {
                row.remove(&shape);
                h.entries = h.entries.saturating_sub(1);
            }
        }
        if row.is_empty() {
            h.table.remove(&place);
        }
    }
}

/// Every argument of every call in `source` that is a call on a plain variable, as its place and its shape (`.name(args)`).
fn scan(source: &str) -> Vec<(Place, String)> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "method_invocation" | "object_creation_expression") {
            if let (Some(name), Some(args)) = (callee_of(node, source), node.child_by_field_name("arguments")) {
                let mut cursor = args.walk();
                let named: Vec<_> = args.named_children(&mut cursor).filter(|c| !matches!(c.kind(), "line_comment" | "block_comment")).collect();
                for (idx, arg) in named.into_iter().enumerate() {
                    if let Some(shape) = shape_of(arg, source) {
                        out.push(((name.clone(), idx), shape));
                    }
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out
}

/// The callee as `call_site_name_and_index` spells it: the method's name, or `new Type`.
fn callee_of(node: Node<'_>, source: &str) -> Option<String> {
    let text = |n: Node<'_>| n.utf8_text(source.as_bytes()).ok().map(str::to_string);
    match node.kind() {
        "method_invocation" => text(node.child_by_field_name("name")?),
        _ => text(node.child_by_field_name("type")?).map(|t| format!("new {}", t.split('<').next().unwrap_or(&t).trim())),
    }
}

/// `.toByteArray()` of `bao.toByteArray()`, when the receiver is a plain lowercase variable.
fn shape_of(arg: Node<'_>, source: &str) -> Option<String> {
    if arg.kind() != "method_invocation" {
        return None;
    }
    let receiver = arg.child_by_field_name("object").filter(|o| o.kind() == "identifier")?;
    let name = receiver.utf8_text(source.as_bytes()).ok()?;
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return None;
    }
    let text = arg.utf8_text(source.as_bytes()).ok()?.trim();
    (text.len() <= MAX_LEN && !text.contains('\n')).then(|| text[name.len()..].to_string())
}

/// The shape of a call on a variable that this place usually has: `(shape, share)`.
fn usual(place: &Place) -> Option<(String, f32)> {
    let h = lock();
    let row = h.table.get(place)?;
    let total: u32 = row.values().sum();
    let key = format!("{}#{}", place.0, place.1);
    let (shape, count) = row.iter().filter(|(shape, _)| !crate::learned_rules::hidden("member_args", &key, shape)).max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
    let share = *count as f32 / total as f32;
    (*count >= MIN_SIGHTINGS && share >= MIN_SHARE).then(|| (shape.clone(), share))
}

/// A variable in scope whose type has the method the shape calls: the nearest one declared, as `(name, declaration)`.
fn nearest_with_method(candidates: &[(String, usize, TypeRef)], method: &str, has_method: &dyn Fn(&TypeRef, &str) -> bool) -> Option<String> {
    candidates.iter().filter(|(name, _, ty)| !name.starts_with('_') && has_method(ty, method)).max_by_key(|(_, decl, _)| *decl).map(|(name, _, _)| name.clone())
}

/// The method a shape calls: `toByteArray` of `.toByteArray()`.
fn method_of(shape: &str) -> &str {
    shape.trim_start_matches('.').split('(').next().unwrap_or("")
}

/// The variables in scope where the caret writes an argument, each with its type: `(name, declaration, type)`.
fn scope_bindings(file: &str, source: &str, offset: usize) -> Option<(Vec<(String, usize, TypeRef)>, std::sync::Arc<dyn TypeResolver + Send + Sync>)> {
    let resolver = IndexService::global().caret_resolver_for(file)?;
    // the buffer as the parser wants it: the call closed, unless the editor already did that
    let closed;
    let text = if source[offset..].trim_start().starts_with(')') {
        source
    } else {
        closed = format!("{});{}", &source[..offset], &source[offset..]);
        &closed
    };
    let sited = format!("{}x{}", &text[..offset], &text[offset..]);
    let tree = parse_java(&sited)?;
    let symbols = extract_symbols(&sited);
    let in_scope = visible_bindings(&tree.root_node(), &sited, &symbols, offset, &*resolver).into_iter().filter_map(|b| Some((b.name, b.decl, b.ty?))).collect();
    Some((in_scope, resolver))
}

/// The simple name of a binary name: `java/util/Map$Entry` → `Entry`.
fn simple_name(binary: &str) -> &str {
    binary.rsplit(['/', '$', '.']).next().unwrap_or(binary)
}

/// The variables in scope at the caret as the argument habits fill their slots from: each with the simple names of its
/// type and of every type above it.
pub(crate) fn scope_candidates(file: &str, source: &str, offset: usize) -> Option<Vec<Candidate>> {
    let (in_scope, resolver) = scope_bindings(file, source, offset)?;
    let brackets = |ty: &TypeRef| "[]".repeat(ty.dims as usize);
    Some(
        in_scope
            .into_iter()
            .map(|(name, decl, ty)| {
                let mut types = vec![format!("{}{}", simple_name(&ty.binary_name), brackets(&ty))];
                if ty.dims == 0 {
                    walk_up(&*resolver as &dyn TypeResolver, &ty, |a| {
                        types.push(simple_name(&a.ty.binary_name).to_string());
                        None::<()>
                    });
                }
                Candidate { name, decl, types }
            })
            .collect(),
    )
}

/// The usual call on a variable for the argument the caret starts, with the variable at hand.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<MemberArg> {
    let (callee, index, start) = bennu_query::prelude::call_site_name_and_index(source, offset)?;
    if !source.get(start..offset)?.trim().is_empty() {
        return None;
    }
    let (shape, share) = usual(&(callee, index))?;
    let (in_scope, resolver) = scope_bindings(file, source, offset)?;
    let has_method = |ty: &TypeRef, method: &str| walk_up(&*resolver as &dyn TypeResolver, ty, |a| a.members.methods.iter().any(|m| m.name == method).then_some(())).is_some();
    let variable = nearest_with_method(&in_scope, method_of(&shape), &has_method)?;
    Some(MemberArg { text: format!("{variable}{shape}"), confidence: share })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(body: &str) -> String {
        format!("class A {{\n    void f() {{\n{body}\n    }}\n}}\n")
    }

    #[test]
    fn a_call_on_a_variable_is_counted_by_its_shape_whatever_the_variable() {
        let found = scan(&method("        store.upload(new ByteArrayInputStream(bao.toByteArray()), doc);"));
        assert!(found.contains(&(("new ByteArrayInputStream".to_string(), 0), ".toByteArray()".to_string())), "{found:?}");
        // a call on a type name or on another call is not a call on a variable
        assert!(scan(&method("        x.m(Foo.make(), a().b());")).is_empty());
    }

    #[test]
    fn the_shape_is_a_habit_only_with_enough_sightings_and_a_majority() {
        for i in 0..6 {
            let var = ["bao", "out", "buffer", "baos", "stream", "b"][i];
            add_file(&format!("ma/U{i}.java"), &method(&format!("        s.up(new ByteArrayInputStream({var}.toByteArray()));")));
        }
        let place = ("new ByteArrayInputStream".to_string(), 0usize);
        let (shape, share) = usual(&place).expect("a habit");
        assert_eq!((shape.as_str(), share), (".toByteArray()", 1.0));
        (0..6).for_each(|i| remove_file(&format!("ma/U{i}.java")));
        assert!(usual(&place).is_none(), "forgotten with its files");
    }

    #[test]
    fn the_nearest_variable_that_has_the_method_is_the_one() {
        let ty = |n: &str| TypeRef::simple(n);
        let in_scope = vec![
            ("early".to_string(), 10, ty("java/io/ByteArrayOutputStream")),
            ("name".to_string(), 50, ty("java/lang/String")),
            ("late".to_string(), 90, ty("java/io/ByteArrayOutputStream")),
            ("_unused".to_string(), 95, ty("java/io/ByteArrayOutputStream")),
        ];
        let has = |t: &TypeRef, m: &str| t.binary_name == "java/io/ByteArrayOutputStream" && m == "toByteArray";
        assert_eq!(nearest_with_method(&in_scope, "toByteArray", &has).as_deref(), Some("late"));
        assert_eq!(nearest_with_method(&in_scope, "size", &has), None);
    }

    #[test]
    fn the_method_of_a_shape() {
        assert_eq!(method_of(".toByteArray()"), "toByteArray");
        assert_eq!(method_of(".get(0)"), "get");
    }
}
