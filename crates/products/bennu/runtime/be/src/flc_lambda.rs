//! The function a call takes, as ghost text: `users.stream().map(▌` → `User::getName`.
//!
//! Two things say what goes there. The **types**: the call's parameter is a function of one argument, and the index knows what that
//! argument is (`User`, for a `Stream<User>`) and which of its methods take nothing and return something. The **project**: each file
//! is read once for the single argument of a call that is a method reference (`User::getName`) or a one-parameter lambda that
//! calls one method on its parameter (`u -> u.getName()`), counted per call (`map`, `filter`, `sorted`) and per method name, and
//! per way of writing it. A method of the argument's type is drawn when the project has passed that name to that call at least
//! [`MIN_SIGHTINGS`] times and it is at least [`MIN_SHARE`] of what it passed among this type's methods, in the way it writes it more often.
//!
//! The types alone would only be a guess among a dozen getters, and the counts alone would name a method the type does not have, so
//! neither is drawn alone.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_java::prelude::{functional_descriptor_at, parse_java, TypeRef};
use tree_sitter::Node;

const MIN_SIGHTINGS: u32 = 2;
const MIN_SHARE: f32 = 0.4;
/// Calls kept; past this the file that is read next is not counted.
const MAX_CALLS: usize = 2_000;

/// A function the project usually passes to a call, and how sure that is.
pub(crate) struct Passed {
    pub text: String,
    pub confidence: f32,
}

/// How a function was written: `User::getName` or `u -> u.getName()`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Style {
    Reference,
    Lambda,
}

#[derive(Default)]
struct Held {
    /// What each file contributed, so it can be taken back out.
    by_file: HashMap<String, Vec<(String, String, Style)>>,
    /// Call name → method name → how it was written → how often.
    table: HashMap<String, HashMap<String, HashMap<Style, u32>>>,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn add_file(file: &str, source: &str) {
    let found = scan(source);
    let mut h = lock();
    remove_locked(&mut h, file);
    for (call, method, style) in &found {
        if h.table.len() >= MAX_CALLS && !h.table.contains_key(call) {
            continue;
        }
        *h.table.entry(call.clone()).or_default().entry(method.clone()).or_default().entry(*style).or_default() += 1;
    }
    h.by_file.insert(file.to_string(), found);
}

fn remove_locked(h: &mut Held, file: &str) {
    for (call, method, style) in h.by_file.remove(file).unwrap_or_default() {
        let Some(row) = h.table.get_mut(&call) else { continue };
        if let Some(styles) = row.get_mut(&method) {
            if let Some(n) = styles.get_mut(&style) {
                *n -= 1;
                if *n == 0 {
                    styles.remove(&style);
                }
            }
            if styles.is_empty() {
                row.remove(&method);
            }
        }
        if row.is_empty() {
            h.table.remove(&call);
        }
    }
}

pub(crate) fn remove_file(file: &str) {
    remove_locked(&mut lock(), file);
}

/// Forgets everything counted, for *Forget what Bennu learned*.
pub(crate) fn clear() {
    *lock() = Held::default();
}

/// Files read, and calls counted, for the memory breakdown.
pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.table.len())
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, Sizes};
    let h = lock();
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.iter().map(|(a, b, _)| text(a) + text(b)).sum());
    }
    s.shared = h.table.iter().map(|(k, row)| text(k) + row.keys().map(|m| text(m)).sum::<usize>()).sum();
    s.shared_entries = h.table.values().map(|r| r.len()).sum();
    s
}

/// The function this project usually passes where the caret is, when it is the argument of a call that takes a function of one
/// argument and the index knows that argument's type.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Passed> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let call = open_call(&source[line_start..offset])?;
    let resolver = crate::index_service::IndexService::global().caret_resolver_for(file)?;
    let descriptor = functional_descriptor_at(source, offset, &*resolver)?;
    let [argument] = descriptor.params.as_slice() else { return None };
    let members = resolver.members_of(&argument.binary_name)?;
    let wants_boolean = matches!(descriptor.returns.binary_name.as_str(), "boolean" | "java/lang/Boolean");
    let candidates: Vec<&str> = members
        .methods
        .iter()
        .filter(|m| !m.is_static && m.params.is_empty() && m.return_type.binary_name != "void")
        .filter(|m| !wants_boolean || matches!(m.return_type.binary_name.as_str(), "boolean" | "java/lang/Boolean"))
        .map(|m| m.name.as_str())
        .collect();
    pick(&call, &candidates, argument)
}

/// The name of the call whose argument list is open at the end of `line_before`: `users.stream().map(` → `map`.
fn open_call(line_before: &str) -> Option<String> {
    let mut depth = 0i32;
    for (i, c) in line_before.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                let name: String = line_before[..i].chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect::<Vec<_>>().into_iter().rev().collect();
                // only the first argument: anything typed after the parenthesis is not an empty slot
                return (!name.is_empty() && line_before[i + 1..].trim().is_empty()).then_some(name);
            }
            _ => {}
        }
    }
    None
}

/// Of the methods of the argument's type, the one the project passes to `call` most, written the way it writes it most.
fn pick(call: &str, candidates: &[&str], argument: &TypeRef) -> Option<Passed> {
    let h = lock();
    let row = h.table.get(call)?;
    let totals: Vec<(&str, u32)> = candidates.iter().filter_map(|m| row.get(*m).map(|s| (*m, s.values().sum()))).collect();
    let all: u32 = totals.iter().map(|t| t.1).sum();
    let &(method, count) = totals.iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))?;
    let share = count as f32 / all as f32;
    if count < MIN_SIGHTINGS || share < MIN_SHARE {
        return None;
    }
    let styles = &row[method];
    let (reference, lambda) = (styles.get(&Style::Reference).copied().unwrap_or(0), styles.get(&Style::Lambda).copied().unwrap_or(0));
    let simple = argument.binary_name.rsplit(['/', '$']).next().unwrap_or(&argument.binary_name);
    let text = if reference >= lambda {
        format!("{simple}::{method}")
    } else {
        let var = simple.chars().next()?.to_lowercase().collect::<String>();
        format!("{var} -> {var}.{method}()")
    };
    Some(Passed { text, confidence: share.min(0.9) })
}

/// Every call of `source` with a single argument that is a method reference to an instance method or a one-parameter lambda that
/// calls one method on its parameter: `(call, method, style)`.
fn scan(source: &str) -> Vec<(String, String, Style)> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "method_invocation" {
            if let Some(found) = passed_to(node, source) {
                out.push(found);
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out
}

fn passed_to(call: Node<'_>, source: &str) -> Option<(String, String, Style)> {
    let text = |n: Node<'_>| n.utf8_text(source.as_bytes()).ok().map(str::to_string);
    let name = text(call.child_by_field_name("name")?)?;
    let arguments = call.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let args: Vec<Node<'_>> = arguments.named_children(&mut cursor).filter(|n| !matches!(n.kind(), "line_comment" | "block_comment")).collect();
    let [only] = args.as_slice() else { return None };
    match only.kind() {
        "method_reference" => {
            let mut c = only.walk();
            let parts: Vec<Node<'_>> = only.named_children(&mut c).collect();
            let [_, method] = parts.as_slice() else { return None };
            let method = text(*method)?;
            (method != "new").then_some((name, method, Style::Reference))
        }
        "lambda_expression" => {
            let params = only.child_by_field_name("parameters")?;
            let var = match params.kind() {
                "identifier" => text(params)?,
                "inferred_parameters" => {
                    let mut c = params.walk();
                    let ids: Vec<Node<'_>> = params.named_children(&mut c).collect();
                    let [one] = ids.as_slice() else { return None };
                    text(*one)?
                }
                _ => return None,
            };
            let body = only.child_by_field_name("body")?;
            if body.kind() != "method_invocation" {
                return None;
            }
            let object = text(body.child_by_field_name("object")?)?;
            let no_args = body.child_by_field_name("arguments")?.named_child_count() == 0;
            (object == var && no_args).then(|| Some((name, text(body.child_by_field_name("name")?)?, Style::Lambda)))?
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// The table is process-wide, so the tests that fill it take turns.
    static TURN: StdMutex<()> = StdMutex::new(());

    fn user() -> TypeRef {
        TypeRef::simple("shop/User")
    }

    fn fill(sources: &[&str]) {
        clear();
        for (i, s) in sources.iter().enumerate() {
            add_file(&format!("/p/F{i}.java"), &format!("class F{i} {{ void f() {{ {s} }} }}"));
        }
    }

    #[test]
    fn references_and_one_call_lambdas_are_counted_per_call_and_method() {
        let found = scan("class A { void f() { xs.stream().map(User::getName); ys.filter(u -> u.isActive()); zs.map(x -> x.a().b()); ws.map(Foo::new); v.map((a, b) -> a.c()); } }");
        let names: Vec<(String, String)> = found.iter().map(|(c, m, _)| (c.clone(), m.clone())).collect();
        assert!(names.contains(&("map".into(), "getName".into())));
        assert!(names.contains(&("filter".into(), "isActive".into())));
        assert_eq!(found.len(), 2, "a chain, a constructor reference and two parameters are not counted: {names:?}");
    }

    #[test]
    fn the_method_the_project_passes_most_is_drawn_the_way_it_writes_it() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(&["xs.map(User::getName);", "ys.map(User::getName);", "zs.map(User::getId);"]);
        let p = pick("map", &["getName", "getId", "getAge"], &user()).expect("two sightings");
        assert_eq!(p.text, "User::getName");
        fill(&["xs.filter(u -> u.isActive());", "ys.filter(x -> x.isActive());"]);
        assert_eq!(pick("filter", &["isActive"], &user()).unwrap().text, "u -> u.isActive()");
    }

    #[test]
    fn a_method_the_type_does_not_have_a_split_vote_or_one_sighting_is_not_drawn() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(&["xs.map(User::getName);", "ys.map(User::getName);"]);
        assert!(pick("map", &["getId"], &user()).is_none(), "the type has no getName");
        assert!(pick("sorted", &["getName"], &user()).is_none(), "another call");
        fill(&["xs.map(User::getName);"]);
        assert!(pick("map", &["getName"], &user()).is_none(), "once");
        fill(&["a.map(U::a);", "b.map(U::b);", "c.map(U::c);", "d.map(U::d);", "e.map(U::e);"]);
        assert!(pick("map", &["a", "b", "c", "d", "e"], &user()).is_none(), "five ways");
    }

    #[test]
    fn a_file_taken_out_takes_its_counts_with_it() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(&["xs.map(User::getName);", "ys.map(User::getName);"]);
        remove_file("/p/F0.java");
        assert!(pick("map", &["getName"], &user()).is_none());
        clear();
        assert_eq!(tracked(), (0, 0));
    }

    #[test]
    fn the_open_call_is_the_one_whose_parenthesis_is_still_open() {
        assert_eq!(open_call("        var names = users.stream().map(").as_deref(), Some("map"));
        assert_eq!(open_call("        foo(a.bar(), baz(").as_deref(), Some("baz"));
        assert_eq!(open_call("        foo(x").as_deref(), None, "something is already typed");
        assert_eq!(open_call("        foo(a).bar").as_deref(), None);
    }
}
