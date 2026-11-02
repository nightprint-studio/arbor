//! What M7 and M8 share: method bodies with the byte span of each statement, the shape of a statement, and a few token-level
//! helpers (which variables a statement declares, assigns, reads).

use std::collections::HashSet;

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use super::m4_methods::{rust_files, Method};
use super::m5_ngram::{abstract_token, tokens};

pub(super) struct Body {
    pub m: Method,
    /// Byte range of every statement, in the file's text.
    pub spans: Vec<(usize, usize)>,
}

fn text<'a>(src: &'a str, n: &Node) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

/// Methods with 1..=`max` statements in `src` (the traversal order of `m4_methods::methods_of`).
pub(super) fn bodies_of(src: &str, file: usize, max: usize) -> Vec<Body> {
    let Some(tree) = parse_java(src) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        let kids: Vec<Node> = n.children(&mut c).collect();
        if n.kind() == "method_declaration" {
            if let (Some(name), Some(body)) = (n.child_by_field_name("name"), n.child_by_field_name("body")) {
                let mut bc = body.walk();
                let stmts: Vec<Node> = body.children(&mut bc).filter(|s| s.is_named() && !s.kind().ends_with("comment")).collect();
                if (1..=max).contains(&stmts.len()) {
                    let mut params = Vec::new();
                    if let Some(list) = n.child_by_field_name("parameters") {
                        let mut pc = list.walk();
                        for p in list.children(&mut pc).filter(|p| p.kind() == "formal_parameter") {
                            if let (Some(ty), Some(nm)) = (p.child_by_field_name("type"), p.child_by_field_name("name")) {
                                params.push((text(src, &ty).to_string(), text(src, &nm).to_string()));
                            }
                        }
                    }
                    let m = Method {
                        name: text(src, &name).to_string(),
                        ret: n.child_by_field_name("type").map(|t| text(src, &t).to_string()).unwrap_or_default(),
                        params,
                        statements: stmts.iter().map(|s| text(src, s).to_string()).collect(),
                        file,
                        start: n.start_byte(),
                        body: (body.start_byte(), body.end_byte()),
                    };
                    out.push(Body { m, spans: stmts.iter().map(|s| (s.start_byte(), s.end_byte())).collect() });
                }
            }
        }
        stack.extend(kids);
    }
    out
}

/// The public training projects' bodies, in a fixed order, with file ids from 1.
pub(super) fn train_bodies(max: usize) -> Vec<Body> {
    let mut out = Vec::new();
    let mut file_ids = 0usize;
    for t in ["commons-io", "commons-collections", "mockito", "junit5", "jackson-databind"] {
        let root = super::m3_rank::assemble_training(t);
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        for f in files {
            file_ids += 1;
            out.extend(bodies_of(&std::fs::read_to_string(&f).unwrap_or_default(), file_ids, max));
        }
    }
    out
}

/// Identifiers → `I`, literals → `L`, keywords and punctuation as they are.
pub(super) fn shape(stmt: &str) -> String {
    tokens(stmt).iter().map(|t| abstract_token(t)).collect::<Vec<_>>().join(" ")
}

fn is_name(t: &str) -> bool {
    abstract_token(t) == "I"
}

fn is_variable(t: &str) -> bool {
    is_name(t) && t.chars().next().is_some_and(|c| c.is_lowercase() || c == '_')
}

/// The variables a statement reads or writes: lower-case names that are neither members (`a.b`) nor called (`f(`).
pub(super) fn variables(stmt: &str) -> HashSet<String> {
    let t = tokens(stmt);
    (0..t.len())
        .filter(|&i| is_variable(t[i]) && (i == 0 || t[i - 1] != ".") && t.get(i + 1) != Some(&"("))
        .map(|i| t[i].to_string())
        .collect()
}

/// The variables a statement defines or updates: `x = …`, `x += …`, `x++`, `++x`.
pub(super) fn defined(stmt: &str) -> HashSet<String> {
    let t = tokens(stmt);
    let mut out = HashSet::new();
    for i in 0..t.len() {
        if !is_variable(t[i]) || (i > 0 && t[i - 1] == ".") { continue }
        let (a, b) = (t.get(i + 1).copied(), t.get(i + 2).copied());
        let assigns = (a == Some("=") && b != Some("=")) || (matches!(a, Some("+" | "-" | "*" | "/" | "%" | "&" | "|" | "^")) && b == Some("=")) || (matches!(a, Some("+" | "-")) && b == a);
        let prefix = i >= 2 && matches!(t[i - 1], "+" | "-") && t[i - 2] == t[i - 1];
        if assigns || prefix { out.insert(t[i].to_string()); }
    }
    out
}

const PRIMITIVES: &[&str] = &["int", "long", "boolean", "double", "float", "char", "byte", "short"];

/// `final Map<K, V> m = …;` → (`Map<K,V>`, `m`).
pub(super) fn declaration(stmt: &str) -> Option<(String, String)> {
    let t = tokens(stmt);
    let mut i = 0;
    while t.get(i) == Some(&"final") { i += 1 }
    let start = i;
    let head = *t.get(i)?;
    if !is_name(head) && !PRIMITIVES.contains(&head) { return None }
    i += 1;
    while t.get(i) == Some(&".") && t.get(i + 1).is_some_and(|x| is_name(x)) { i += 2 }
    if t.get(i) == Some(&"<") {
        let mut depth = 0;
        while let Some(&x) = t.get(i) {
            depth += i32::from(x == "<") - i32::from(x == ">");
            i += 1;
            if depth == 0 { break }
        }
    }
    while t.get(i) == Some(&"[") && t.get(i + 1) == Some(&"]") { i += 2 }
    let name = *t.get(i)?;
    if !is_name(name) || !matches!(t.get(i + 1), Some(&"=") | Some(&";")) || (t.get(i + 1) == Some(&"=") && t.get(i + 2) == Some(&"=")) { return None }
    Some((t[start..i].concat(), name.to_string()))
}

/// `Map<K,V>` / `java.util.List` / `String[]` → `Map` / `List` / `String`.
pub(super) fn simple_type(t: &str) -> &str {
    t.split('<').next().unwrap_or(t).trim_end_matches("[]").trim_end_matches("...").rsplit('.').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_carry_the_span_of_each_statement() {
        let src = "class A { int f(int a) { int b = a;\n return b; } }";
        let bs = bodies_of(src, 1, 8);
        assert_eq!(bs.len(), 1);
        assert_eq!(&src[bs[0].spans[1].0..bs[0].spans[1].1], "return b;");
    }

    #[test]
    fn variables_defined_and_read_are_told_apart() {
        assert!(defined("total += item.price();").contains("total"));
        assert!(!defined("if (a == b) return;").contains("a"));
        assert!(defined("i++;").contains("i"));
        let v = variables("x = foo(y).bar;");
        assert!(v.contains("x") && v.contains("y") && !v.contains("foo") && !v.contains("bar"));
    }

    #[test]
    fn a_declaration_gives_its_type_and_its_name() {
        assert_eq!(declaration("final Map<String, List<Integer>> m = new HashMap<>();"), Some(("Map<String,List<Integer>>".to_string(), "m".to_string())));
        assert_eq!(declaration("int[] xs = f();"), Some(("int[]".to_string(), "xs".to_string())));
        assert_eq!(declaration("x = 3;"), None);
        assert_eq!(declaration("a.b = c;"), None);
        assert_eq!(simple_type("java.util.Map<K,V>"), "Map");
    }
}
