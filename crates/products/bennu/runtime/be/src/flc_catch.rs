//! What a project does when it catches an exception: the first statement of a `catch` block, on the empty line after the `{`.
//!
//! After `catch (SQLException e) {` this project's first line is `throw new DataAccessException("query failed", e);` nearly every
//! time, and nothing on the empty line says so. Each file is read once for the first statement of every `catch` that has one, keyed by
//! the exception's simple type, with the caught variable's name taken out of the text so the same line counts whatever it is called
//! there. A statement is drawn when it is at least [`MIN_SHARE`] of what that type was handled with, at least [`MIN_SIGHTINGS`] times,
//! with the name the block in front of the caret gives the variable.
//!
//! A `catch` that swallows (an empty block) is never offered: not writing anything is the one habit that is better left unsuggested.
//! A multi-catch has no one type to key by, and a statement over several lines is not a line to copy.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

const MIN_SIGHTINGS: u32 = 3;
const MIN_SHARE: f32 = 0.6;
/// Longest statement worth counting.
const MAX_LEN: usize = 140;
/// Stands for the caught variable inside a counted statement.
const SLOT: &str = "\u{1}";
/// Exception types kept; past this the file that is read next is not counted.
const MAX_KEYS: usize = 5_000;

/// A statement the project writes first in a `catch`, and how sure that is.
pub(crate) struct Handling {
    pub text: String,
    pub confidence: f32,
}

#[derive(Default)]
struct Held {
    /// What each file contributed, so it can be taken back out.
    by_file: HashMap<String, Vec<(String, String)>>,
    /// Exception type → statement (variable as [`SLOT`]) → how often.
    table: HashMap<String, HashMap<String, u32>>,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn add_file(file: &str, source: &str) {
    let pairs = scan(source);
    let mut h = lock();
    remove_locked(&mut h, file);
    for (ty, stmt) in &pairs {
        if h.table.len() >= MAX_KEYS && !h.table.contains_key(ty) {
            continue;
        }
        *h.table.entry(ty.clone()).or_default().entry(stmt.clone()).or_default() += 1;
    }
    h.by_file.insert(file.to_string(), pairs);
}

fn remove_locked(h: &mut Held, file: &str) {
    for (ty, stmt) in h.by_file.remove(file).unwrap_or_default() {
        if let Some(row) = h.table.get_mut(&ty) {
            if let Some(n) = row.get_mut(&stmt) {
                *n -= 1;
                if *n == 0 {
                    row.remove(&stmt);
                }
            }
            if row.is_empty() {
                h.table.remove(&ty);
            }
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

/// Files read, and exception types counted, for the memory breakdown.
pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.table.len())
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, two_level, Sizes};
    let h = lock();
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.iter().map(|(a, b)| text(a) + text(b)).sum());
    }
    s.shared = two_level(&h.table, |k| text(k));
    s.shared_entries = h.table.values().map(|r| r.len()).sum();
    s
}

/// The statement this project usually writes first when it catches the exception of the `catch` the caret is at the start of.
pub(crate) fn suggest(source: &str, offset: usize) -> Option<Handling> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let tree = parse_java(source)?;
    let at = tree.root_node().descendant_for_byte_range(offset, offset)?;
    let block = std::iter::successors(Some(at), |n| n.parent()).find(|n| n.kind() == "block")?;
    let clause = block.parent().filter(|c| c.kind() == "catch_clause")?;
    // the first statement: nothing but comments between the brace and the caret
    let mut cursor = block.walk();
    if block.named_children(&mut cursor).any(|s| !is_comment(s) && s.start_byte() < offset) {
        return None;
    }
    let (ty, name) = caught(clause, source)?;
    let h = lock();
    let row = h.table.get(&ty)?;
    let total: u32 = row.values().sum();
    let (stmt, count) = row.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
    let share = *count as f32 / total as f32;
    (*count >= MIN_SIGHTINGS && share >= MIN_SHARE).then(|| Handling { text: stmt.replace(SLOT, &name), confidence: share })
}

fn is_comment(n: Node<'_>) -> bool {
    matches!(n.kind(), "line_comment" | "block_comment")
}

/// `(simple type, variable name)` of a `catch (Type name)`; `None` for a multi-catch.
fn caught(clause: Node<'_>, source: &str) -> Option<(String, String)> {
    let mut c = clause.walk();
    let param = clause.named_children(&mut c).find(|n| n.kind() == "catch_formal_parameter")?;
    let mut k = param.walk();
    let types: Vec<Node<'_>> = param.named_children(&mut k).filter(|n| n.kind() == "catch_type").collect();
    let [only] = types.as_slice() else { return None };
    let mut t = only.walk();
    let mut kinds = only.named_children(&mut t);
    let ty = kinds.next()?;
    if kinds.next().is_some() {
        return None;
    }
    let ty = ty.utf8_text(source.as_bytes()).ok()?;
    let ty = ty.split('<').next().unwrap_or(ty).rsplit('.').next().unwrap_or(ty).trim().to_string();
    let name = param.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok()?.to_string();
    Some((ty, name))
}

/// The first statement of every `catch` of `source` that has one, with its exception type.
fn scan(source: &str) -> Vec<(String, String)> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "catch_clause" {
            if let Some(pair) = first_statement(node, source) {
                out.push(pair);
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out
}

fn first_statement(clause: Node<'_>, source: &str) -> Option<(String, String)> {
    let (ty, name) = caught(clause, source)?;
    let body = clause.child_by_field_name("body")?;
    let mut cursor = body.walk();
    let stmt = body.named_children(&mut cursor).find(|s| !is_comment(*s))?;
    let text = stmt.utf8_text(source.as_bytes()).ok()?;
    if text.contains('\n') || text.len() > MAX_LEN {
        return None;
    }
    Some((ty, with_slot(text, &name)))
}

/// `text` with every whole-word `name` replaced by [`SLOT`].
fn with_slot(text: &str, name: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(name) {
        let before = rest[..i].chars().next_back();
        let after = rest[i + name.len()..].chars().next();
        out.push_str(&rest[..i]);
        if before.is_none_or(|c| !word(c)) && after.is_none_or(|c| !word(c)) {
            out.push_str(SLOT);
        } else {
            out.push_str(name);
        }
        rest = &rest[i + name.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// The table is process-wide, so the tests that fill it take turns.
    static TURN: StdMutex<()> = StdMutex::new(());

    fn file(i: usize, var: &str, body: &str) -> String {
        format!("class F{i} {{\n void f() {{\n  try {{ g(); }} catch (SQLException {var}) {{\n   {body}\n  }}\n }}\n}}\n")
    }

    fn fill(n: usize, var: &str) {
        clear();
        for i in 0..n {
            add_file(&format!("/p/F{i}.java"), &file(i, var, &format!("throw new DataAccessException(\"failed\", {var});")));
        }
    }

    fn at(var: &str) -> Option<Handling> {
        let source = format!("class T {{\n void f() {{\n  try {{ g(); }} catch (SQLException {var}) {{\n   \n  }}\n }}\n}}\n");
        let offset = source.find("   \n").unwrap() + 3;
        suggest(&source, offset)
    }

    #[test]
    fn the_usual_handling_is_drawn_with_the_name_the_block_gives_the_variable() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(3, "e");
        let h = at("ex").expect("seen three times");
        assert_eq!(h.text, "throw new DataAccessException(\"failed\", ex);");
        assert!((h.confidence - 1.0).abs() < 1e-6);
    }

    #[test]
    fn another_exception_a_split_vote_or_too_few_sightings_get_nothing() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(2, "e");
        assert!(at("e").is_none(), "two sightings");
        fill(3, "e");
        let other = "class T {\n void f() {\n  try { g(); } catch (IOException e) {\n   \n  }\n }\n}\n";
        assert!(suggest(other, other.find("   \n").unwrap() + 3).is_none(), "never seen IOException");
        clear();
        for (i, body) in ["a(e);", "b(e);", "c(e);", "d(e);"].iter().enumerate() {
            add_file(&format!("/p/G{i}.java"), &file(i, "e", body));
        }
        assert!(at("e").is_none(), "four ways, none at 60%");
    }

    #[test]
    fn a_swallowing_catch_a_multi_catch_and_a_later_statement_are_left_alone() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        for i in 0..4 {
            add_file(&format!("/p/S{i}.java"), "class S { void f() { try { g(); } catch (SQLException e) { } } }");
        }
        assert!(at("e").is_none(), "an empty catch teaches nothing to copy");
        fill(3, "e");
        let multi = "class T {\n void f() {\n  try { g(); } catch (SQLException | IOException e) {\n   \n  }\n }\n}\n";
        assert!(suggest(multi, multi.find("   \n").unwrap() + 3).is_none());
        let after = "class T {\n void f() {\n  try { g(); } catch (SQLException e) {\n   h(e);\n   \n  }\n }\n}\n";
        assert!(suggest(after, after.rfind("   \n").unwrap() + 3).is_none(), "only the first statement");
    }

    #[test]
    fn a_file_taken_out_takes_its_counts_with_it() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(3, "e");
        remove_file("/p/F0.java");
        assert!(at("e").is_none());
        clear();
        assert_eq!(tracked(), (0, 0));
    }

    #[test]
    fn only_the_whole_word_is_the_variable() {
        assert_eq!(with_slot("throw new X(e.getMessage(), e);", "e"), "throw new X(\u{1}.getMessage(), \u{1});");
        assert_eq!(with_slot("handle(exception, e1, e);", "e"), "handle(exception, e1, \u{1});");
    }
}
