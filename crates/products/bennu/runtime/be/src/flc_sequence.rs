//! What usually comes next: the statement a project writes after another one, offered on an empty line.
//!
//! After `bao = new ByteArrayOutputStream();` this project's next line is `jaxb_processor.sneaky_marshal(` nearly every
//! time, and nothing on the empty line says so. Each file is read once for every pair of neighbouring statements in a
//! block: what the first one is (the method it calls, the type it declares, an assignment) and how the second one starts
//! (`receiver.method(`). The counts are summed over the files, and a start is drawn when it follows that kind of statement
//! in at least [`MIN_SHARE`] of the sightings, at least [`MIN_SIGHTINGS`] times.
//!
//! Only the beginning of the call is drawn, never its arguments: those are the other layers' business, and they take over
//! from the opening parenthesis. A project's receivers (`storage_util`, `this.mapper`) are what makes the start worth
//! drawing, so a receiver that is a local variable of some other method simply never reaches the share.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

const MIN_SIGHTINGS: u32 = 3;
const MIN_SHARE: f32 = 0.6;
/// Longest start worth counting; a receiver chain longer than this is not a habit of the project.
const MAX_HEAD: usize = 60;
/// Followed statements kept; past this the file that is read next is not counted.
const MAX_KEYS: usize = 20_000;

/// A start the project writes after some statement, and how sure that is.
pub(crate) struct Sequel {
    pub text: String,
    pub confidence: f32,
}

#[derive(Default)]
struct Held {
    /// What each file contributed, so it can be taken back out.
    by_file: HashMap<String, Vec<(String, String)>>,
    /// Statement kind → how the next one starts → how often.
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
    for (key, head) in &pairs {
        if h.table.len() >= MAX_KEYS && !h.table.contains_key(key) {
            continue;
        }
        *h.table.entry(key.clone()).or_default().entry(head.clone()).or_default() += 1;
    }
    h.by_file.insert(file.to_string(), pairs);
}

/// What follows what, for the page that shows it: per kind of statement, how the next one started and how often.
pub(crate) fn report() -> Vec<(String, Vec<(String, u32)>)> {
    let h = lock();
    let mut out: Vec<(String, Vec<(String, u32)>)> = h
        .table
        .iter()
        .map(|(key, row)| {
            let mut rows: Vec<(String, u32)> = row.iter().map(|(t, n)| (t.clone(), *n)).collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            (key.clone(), rows)
        })
        .collect();
    out.sort_by_key(|(key, rows)| (std::cmp::Reverse(rows.iter().map(|r| r.1).sum::<u32>()), key.clone()));
    out
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

/// Forgets everything counted, for *Forget what Bennu learned*.
pub(crate) fn clear() {
    *lock() = Held::default();
}

/// Files read, and statement kinds counted, for the memory breakdown.
pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.table.len())
}

pub(crate) fn remove_file(file: &str) {
    remove_locked(&mut lock(), file);
}

fn remove_locked(h: &mut Held, file: &str) {
    for (key, head) in h.by_file.remove(file).unwrap_or_default() {
        if let Some(row) = h.table.get_mut(&key) {
            if let Some(n) = row.get_mut(&head) {
                *n -= 1;
                if *n == 0 {
                    row.remove(&head);
                }
            }
            if row.is_empty() {
                h.table.remove(&key);
            }
        }
    }
}

/// The start of the statement that usually follows the one above the caret, for a caret on an empty line inside a block.
pub(crate) fn suggest(source: &str, offset: usize) -> Option<Sequel> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let tree = parse_java(source)?;
    let at = tree.root_node().descendant_for_byte_range(offset, offset)?;
    let block = std::iter::successors(Some(at), |n| n.parent()).find(|n| matches!(n.kind(), "block" | "constructor_body"))?;
    let above = statements(block).into_iter().rfind(|s| s.end_byte() <= offset)?;
    // the statement right above, not one with another between it and the caret
    if statements(block).into_iter().any(|s| s.start_byte() >= offset && s.start_byte() < offset) {
        return None;
    }
    let key = key_of(above, source)?;
    let h = lock();
    let row = h.table.get(&key)?;
    let total: u32 = row.values().sum();
    let (head, count) = row.iter().filter(|(head, _)| !crate::learned_rules::hidden("sequence", &key, head)).max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
    let share = *count as f32 / total as f32;
    (*count >= MIN_SIGHTINGS && share >= MIN_SHARE).then(|| Sequel { text: head.clone(), confidence: share })
}

/// Every pair of neighbouring statements of every block of `source`: what the first is, and how the second starts.
fn scan(source: &str) -> Vec<(String, String)> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "block" | "constructor_body") {
            for pair in statements(node).windows(2) {
                if let (Some(key), Some(head)) = (key_of(pair[0], source), head_of(pair[1], source)) {
                    out.push((key, head));
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out
}

/// The statements directly in a block, comments left out.
fn statements(block: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = block.walk();
    block.named_children(&mut cursor).filter(|n| !matches!(n.kind(), "line_comment" | "block_comment")).collect()
}

/// What kind of statement this is, as the project's habits are keyed: the method an expression calls, the type a local
/// declares, or an assignment. Statements that say nothing about what comes next (a `return`, a loop) have no key.
fn key_of(stmt: Node<'_>, source: &str) -> Option<String> {
    let text = |n: Node<'_>| n.utf8_text(source.as_bytes()).ok().map(str::to_string);
    match stmt.kind() {
        "expression_statement" => {
            let expr = stmt.named_child(0)?;
            match expr.kind() {
                "method_invocation" => Some(format!("call:{}", crate::flc_words::canon(&text(expr.child_by_field_name("name")?)?))),
                "assignment_expression" => Some("assign".to_string()),
                _ => None,
            }
        }
        "local_variable_declaration" => {
            let ty = text(stmt.child_by_field_name("type")?)?;
            let ty = ty.split('<').next().unwrap_or(&ty).trim();
            // `var x = new Foo()` declares a `Foo` and `var x = repo.load()` is the result of a call: the keyword says neither
            if matches!(ty, "var" | "val") {
                let init = stmt.named_children(&mut stmt.walk()).find(|n| n.kind() == "variable_declarator")?.child_by_field_name("value")?;
                return match init.kind() {
                    "object_creation_expression" => {
                        let made = text(init.child_by_field_name("type")?)?;
                        Some(format!("decl:{}", made.split('<').next().unwrap_or(&made).trim()))
                    }
                    "method_invocation" => Some(format!("init:{}", crate::flc_words::canon(&text(init.child_by_field_name("name")?)?))),
                    _ => None,
                };
            }
            Some(format!("decl:{ty}"))
        }
        _ => None,
    }
}

/// How a statement starts when it is a call on a plain receiver: `receiver.method(`, or `method(`.
fn head_of(stmt: Node<'_>, source: &str) -> Option<String> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let call = stmt.named_child(0).filter(|e| e.kind() == "method_invocation")?;
    let name = call.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok()?;
    let head = match call.child_by_field_name("object") {
        Some(receiver) if matches!(receiver.kind(), "identifier" | "this" | "field_access") => {
            format!("{}.{name}(", receiver.utf8_text(source.as_bytes()).ok()?)
        }
        Some(_) => return None,
        None => format!("{name}("),
    };
    (head.len() <= MAX_HEAD).then_some(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(body: &str) -> String {
        format!("class A {{\n    void f() {{\n{body}\n    }}\n}}\n")
    }

    fn project() {
        for i in 0..4 {
            add_file(
                &format!("seq/S{i}.java"),
                &method("        var bao = new ByteArrayOutputStream();\n        jaxb_processor.marshal(x, bao);\n        other();"),
            );
        }
    }

    #[test]
    fn the_start_that_follows_a_kind_of_statement_is_drawn_on_the_empty_line() {
        project();
        let src = method("        var bao = new ByteArrayOutputStream();\n        ");
        let got = suggest(&src, src.len() - "\n    }\n}\n".len()).expect("a sequel");
        assert_eq!(got.text, "jaxb_processor.marshal(");
        assert!(got.confidence >= 0.6);
        (0..4).for_each(|i| remove_file(&format!("seq/S{i}.java")));
    }

    /// A probe, not a test: `FLC_SEQ_PROBE=<dir> cargo test -p bennu-be probe_a_project -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn probe_a_project() {
        let Ok(dir) = std::env::var("FLC_SEQ_PROBE") else { return };
        let (mut files, mut pairs) = (0, 0);
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "java") {
                    let text = String::from_utf8_lossy(&std::fs::read(&p).unwrap()).into_owned();
                    pairs += scan(&text).len();
                    add_file(&p.to_string_lossy(), &text);
                    files += 1;
                }
            }
        }
        let (tracked, kinds) = tracked();
        let usable = report().iter().filter(|(_, rows)| rows.iter().map(|r| r.1).sum::<u32>() >= MIN_SIGHTINGS).count();
        println!("files {files}, pairs {pairs}, kinds {kinds} (files tracked {tracked}), kinds with >= {MIN_SIGHTINGS} sightings {usable}");
    }

    #[test]
    fn a_statement_kind_is_the_same_in_every_case() {
        // two files spell the call one way, two the other: the habit is only there when they count together
        for (i, call) in ["opener.openStream();", "opener.open_stream();", "opener.openStream();", "opener.open_stream();"].iter().enumerate() {
            add_file(&format!("seq/case{i}.java"), &method(&format!("        {call}\n        zzconsumer.take(x);\n        other();")));
        }
        let src = method("        opener.openStream();\n        ");
        let got = suggest(&src, src.len() - "\n    }\n}\n".len()).expect("a sequel");
        assert_eq!(got.text, "zzconsumer.take(");
        (0..4).for_each(|i| remove_file(&format!("seq/case{i}.java")));
    }

    #[test]
    fn too_few_sightings_give_nothing_and_a_removed_file_is_forgotten() {
        add_file("seq/Lone.java", &method("        var q = new Quux();\n        r.go();"));
        let src = method("        var q = new Quux();\n        ");
        assert!(suggest(&src, src.len() - "\n    }\n}\n".len()).is_none());
        remove_file("seq/Lone.java");
    }
}
