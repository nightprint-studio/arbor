//! What a test checks after it computes something: the assertion the project writes under a local variable, in a test file.
//!
//! After `var result = order.total();` this project's next line is `assertThat(result).isEqualTo(` nearly every time, whichever method
//! was called, and nothing on the empty line says so. Each test file is read once for the assertion statement that comes right after
//! a local declaration and mentions the variable; the variable is taken out of it, and the statement is cut at its first argument
//! that is not the variable — the value expected is the one thing the person writing the test has to supply. A shape is drawn
//! when it is at least [`MIN_SHARE`] of what the project wrote there, at least [`MIN_SIGHTINGS`] times, with the variable's name
//! from the declaration above the caret.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

const MIN_SIGHTINGS: u32 = 3;
const MIN_SHARE: f32 = 0.5;
const MAX_LEN: usize = 160;
/// Shapes kept; past this the file that is read next is not counted.
const MAX_SHAPES: usize = 2_000;
/// Stands for the variable inside a counted statement.
const SLOT: &str = "\u{1}";
/// A call whose name starts with one of these checks something.
const CHECKS: [&str; 5] = ["assert", "verify", "then", "expect", "should"];

/// The assertion the project usually writes there, and how sure that is.
pub(crate) struct Assertion {
    pub text: String,
    pub confidence: f32,
}

#[derive(Default)]
struct Held {
    /// What each file contributed, so it can be taken back out.
    by_file: HashMap<String, Vec<String>>,
    shapes: HashMap<String, u32>,
}

fn lock() -> MutexGuard<'static, Held> {
    static HELD: OnceLock<Mutex<Held>> = OnceLock::new();
    HELD.get_or_init(Mutex::default).lock().unwrap_or_else(|p| p.into_inner())
}

/// Whether `file` is a test: under `src/test`, or named like one.
fn is_test_file(file: &str) -> bool {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = name.strip_suffix(".java").unwrap_or(name);
    file.contains("/src/test/") || file.contains("\\src\\test\\") || stem.ends_with("Test") || stem.ends_with("Tests") || stem.ends_with("IT")
}

pub(crate) fn add_file(file: &str, source: &str) {
    if !is_test_file(file) {
        return;
    }
    let found = scan(source);
    let mut h = lock();
    remove_locked(&mut h, file);
    for shape in &found {
        if h.shapes.len() >= MAX_SHAPES && !h.shapes.contains_key(shape) {
            continue;
        }
        *h.shapes.entry(shape.clone()).or_default() += 1;
    }
    h.by_file.insert(file.to_string(), found);
}

fn remove_locked(h: &mut Held, file: &str) {
    for shape in h.by_file.remove(file).unwrap_or_default() {
        if let Some(n) = h.shapes.get_mut(&shape) {
            *n -= 1;
            if *n == 0 {
                h.shapes.remove(&shape);
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

/// Files read, and shapes counted, for the memory breakdown.
pub(crate) fn tracked() -> (usize, usize) {
    let h = lock();
    (h.by_file.len(), h.shapes.len())
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, Sizes};
    let h = lock();
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.iter().map(|a| text(a)).sum());
    }
    s.shared = h.shapes.keys().map(|k| text(k)).sum();
    s.shared_entries = h.shapes.len();
    s
}

/// The assertion this project usually writes under the local declaration above the caret, in a test file.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Assertion> {
    if !is_test_file(file) || !source.is_char_boundary(offset) {
        return None;
    }
    let tree = parse_java(source)?;
    let at = tree.root_node().descendant_for_byte_range(offset, offset)?;
    let block = std::iter::successors(Some(at), |n| n.parent()).find(|n| n.kind() == "block")?;
    let mut cursor = block.walk();
    let above = block.named_children(&mut cursor).filter(|n| !is_comment(*n)).filter(|n| n.end_byte() <= offset).last()?;
    // the statement right above the caret, and nothing between it and the caret but blanks
    if !source[above.end_byte()..offset].trim().is_empty() {
        return None;
    }
    let var = declared(above, source)?;
    let h = lock();
    let total: u32 = h.shapes.values().sum();
    let (shape, count) = h.shapes.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
    let share = *count as f32 / total as f32;
    (*count >= MIN_SIGHTINGS && share >= MIN_SHARE).then(|| Assertion { text: shape.replace(SLOT, &var), confidence: share })
}

fn is_comment(n: Node<'_>) -> bool {
    matches!(n.kind(), "line_comment" | "block_comment")
}

/// The name a local declaration with one variable gives it.
fn declared(stmt: Node<'_>, source: &str) -> Option<String> {
    if stmt.kind() != "local_variable_declaration" {
        return None;
    }
    let mut c = stmt.walk();
    let decls: Vec<Node<'_>> = stmt.named_children(&mut c).filter(|n| n.kind() == "variable_declarator").collect();
    let [one] = decls.as_slice() else { return None };
    one.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok().map(str::to_string)
}

/// The shape of every assertion that comes right after a declaration and mentions what it declares.
fn scan(source: &str) -> Vec<String> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "block" {
            let mut cursor = node.walk();
            let stmts: Vec<Node<'_>> = node.named_children(&mut cursor).filter(|n| !is_comment(*n)).collect();
            for pair in stmts.windows(2) {
                if let Some(var) = declared(pair[0], source) {
                    out.extend(shape_of(pair[1], source, &var));
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out
}

/// A checking statement with `var` taken out and cut before its first argument that is not `var`.
fn shape_of(stmt: Node<'_>, source: &str, var: &str) -> Option<String> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let call = stmt.named_child(0).filter(|e| e.kind() == "method_invocation")?;
    let name = call.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok()?.to_lowercase();
    // either end of the chain names the check: `assertThat(x).isEqualTo(y)` starts with it, `then(x).should()` ends with it
    let first = leftmost_call_name(call, source)?.to_lowercase();
    if !CHECKS.iter().any(|c| name.starts_with(c) || first.starts_with(c)) {
        return None;
    }
    let text = stmt.utf8_text(source.as_bytes()).ok()?;
    if text.contains('\n') || text.len() > MAX_LEN {
        return None;
    }
    let slotted = with_slot(text, var);
    if !slotted.contains(SLOT) {
        return None;
    }
    Some(cut_at_first_value(&slotted))
}

/// The name of the first call of a chain: `assertThat` of `assertThat(x).isEqualTo(y)`.
fn leftmost_call_name(call: Node<'_>, source: &str) -> Option<String> {
    let mut node = call;
    while let Some(object) = node.child_by_field_name("object").filter(|o| o.kind() == "method_invocation") {
        node = object;
    }
    node.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok().map(str::to_string)
}

/// `text` with every whole-word `var` replaced by [`SLOT`].
fn with_slot(text: &str, var: &str) -> String {
    if var.is_empty() {
        return text.to_string();
    }
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let (mut out, mut rest) = (String::with_capacity(text.len()), text);
    while let Some(i) = rest.find(var) {
        let before = rest[..i].chars().next_back();
        let after = rest[i + var.len()..].chars().next();
        out.push_str(&rest[..i]);
        out.push_str(if before.is_none_or(|c| !word(c)) && after.is_none_or(|c| !word(c)) { SLOT } else { var });
        rest = &rest[i + var.len()..];
    }
    out.push_str(rest);
    out
}

/// The statement up to its first argument that is a value of the person's choosing: a literal, or a name that is not the slot.
fn cut_at_first_value(text: &str) -> String {
    let toks = crate::flc_ngram::tokens(text);
    let mut prev = "";
    for t in &toks {
        let at_argument = matches!(prev, "(" | ",");
        let first = t.as_bytes()[0];
        let is_value = t.starts_with('"') || t.starts_with('\'') || first.is_ascii_digit() || (first.is_ascii_alphabetic() && *t != SLOT && !matches!(*t, "new" | "null" | "true" | "false"));
        if at_argument && is_value {
            let end = t.as_ptr() as usize - text.as_ptr() as usize;
            return text[..end].trim_end().to_string();
        }
        prev = t;
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// The table is process-wide, so the tests that fill it take turns.
    static TURN: StdMutex<()> = StdMutex::new(());

    fn test_file(i: usize, body: &str) -> String {
        format!("class F{i}Test {{\n @Test void t() {{\n  {body}\n }}\n}}\n")
    }

    fn fill(bodies: &[&str]) {
        clear();
        for (i, b) in bodies.iter().enumerate() {
            add_file(&format!("/p/src/test/java/F{i}Test.java"), &test_file(i, b));
        }
    }

    fn at(var_decl: &str) -> Option<Assertion> {
        let source = format!("class T {{\n @Test void t() {{\n  {var_decl}\n  \n }}\n}}\n");
        suggest("/p/src/test/java/TTest.java", &source, source.rfind("  \n").unwrap() + 2)
    }

    #[test]
    fn the_assertion_is_cut_before_the_value_the_person_has_to_supply() {
        assert_eq!(cut_at_first_value("assertThat(\u{1}).isEqualTo(expected);"), "assertThat(\u{1}).isEqualTo(");
        assert_eq!(cut_at_first_value("assertEquals(42, \u{1});"), "assertEquals(");
        assert_eq!(cut_at_first_value("assertNotNull(\u{1});"), "assertNotNull(\u{1});");
        assert_eq!(cut_at_first_value("assertThat(\u{1}).isNotNull();"), "assertThat(\u{1}).isNotNull();");
    }

    #[test]
    fn the_usual_assertion_is_drawn_with_the_name_the_declaration_gives() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(&[
            "var result = a.total(); assertThat(result).isEqualTo(5);",
            "int n = b.count(); assertThat(n).isEqualTo(2);",
            "String s = c.name(); assertThat(s).isEqualTo(\"x\");",
        ]);
        let a = at("var total = order.total();").expect("seen three times");
        assert_eq!(a.text, "assertThat(total).isEqualTo(");
        assert!((a.confidence - 1.0).abs() < 1e-6);
    }

    #[test]
    fn only_a_test_a_declaration_above_and_enough_sightings_get_it() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(&["var r = a.f(); assertThat(r).isNotNull();", "var r = a.g(); assertThat(r).isNotNull();"]);
        assert!(at("var r = a.f();").is_none(), "two sightings");
        fill(&["var r = a.f(); assertThat(r).isNotNull();", "var r = a.g(); assertThat(r).isNotNull();", "var r = a.h(); assertThat(r).isNotNull();"]);
        assert!(at("a.f();").is_none(), "no declaration above");
        let source = "class T {\n void t() {\n  var r = a.f();\n  \n }\n}\n";
        assert!(suggest("/p/src/main/java/T.java", source, source.rfind("  \n").unwrap() + 2).is_none(), "not a test file");
    }

    #[test]
    fn an_assertion_that_ignores_the_variable_or_a_plain_call_is_not_counted() {
        assert!(scan("class A { void t() { var r = f(); assertTrue(g()); var q = f(); h(q); } }").is_empty());
        assert_eq!(scan("class A { void t() { var r = f(); verify(r); } }"), ["verify(\u{1});"]);
    }

    #[test]
    fn a_file_taken_out_takes_its_counts_with_it() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        fill(&["var r = a.f(); assertThat(r).isNotNull();"; 3]);
        remove_file("/p/src/test/java/F0Test.java");
        assert!(at("var r = a.f();").is_none());
        clear();
        assert_eq!(tracked(), (0, 0));
    }
}
