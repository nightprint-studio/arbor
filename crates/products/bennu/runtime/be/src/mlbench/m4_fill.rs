//! M4, the second half: fill the retrieved skeleton for the method being written, and let Bennu's checker say whether it compiles.
//!
//! For a held-out method (name, signature, the fields of its class known; the body unknown) the five most similar methods of the same
//! arity are taken and each is *adapted*: the identifiers of its body are mapped onto the target's — its i-th parameter becomes the
//! target's i-th, the property its name derives (`getX` → `x`) becomes the target's, and any other free name is filled from the
//! target's parameters and fields in the order the production ranker (`flc_rank`) gives for the name that stands there. The first
//! candidate whose body adds no error that the original body did not already have wins.
//!
//! `cargo test -p bennu-be mlbench::m4_fill -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project, default 200)

use std::collections::{BTreeMap, HashSet};

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use super::common::*;
use super::m4_methods::{class_of, methods_of, normalize, rust_files, skeleton, subwords, Corpus, Method};
use super::m5_ngram::{abstract_token, tokens};
use crate::flc_rank::{self, Row};
use crate::index_service::IndexService;

/// `getName` / `isActive` / `hasItems` / `setName` → `name` / `active` / `items` / `name`.
pub(super) fn property_of(method: &str) -> Option<String> {
    let rest = ["get", "is", "has", "set"].iter().find_map(|p| method.strip_prefix(p))?;
    let mut c = rest.chars();
    let first = c.next().filter(|f| f.is_uppercase())?;
    Some(format!("{}{}", first.to_lowercase(), c.as_str()))
}

/// The fields of the class that holds the method starting at `method_start`: (type, name).
pub(super) fn class_fields(src: &str, method_start: usize) -> Vec<(String, String)> {
    let Some(tree) = parse_java(src) else { return Vec::new() };
    let mut node: Node = match tree.root_node().descendant_for_byte_range(method_start, method_start) {
        Some(n) => n,
        None => return Vec::new(),
    };
    while node.kind() != "class_body" && node.kind() != "enum_body" {
        match node.parent() {
            Some(p) => node = p,
            None => return Vec::new(),
        }
    }
    let mut out = Vec::new();
    let mut c = node.walk();
    for f in node.children(&mut c).filter(|f| f.kind() == "field_declaration") {
        let ty = f.child_by_field_name("type").map(|t| src[t.start_byte()..t.end_byte()].to_string()).unwrap_or_default();
        let mut c2 = f.walk();
        for d in f.children(&mut c2).filter(|d| d.kind() == "variable_declarator") {
            if let Some(n) = d.child_by_field_name("name") {
                out.push((ty.clone(), src[n.start_byte()..n.end_byte()].to_string()));
            }
        }
    }
    out
}

fn simple(t: &str) -> &str {
    t.split('<').next().unwrap_or(t).rsplit('.').next().unwrap_or(t).trim()
}

/// Names a statement declares (`Foo x = …`, `for (Foo x : …)`): they are the body's own and are kept.
fn declared_locals(stmts: &[String]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        let t = tokens(s);
        for w in t.windows(3) {
            if abstract_token(w[0]) != "L" && (abstract_token(w[0]) == "I" || w[0] == "int" || w[0] == "long" || w[0] == "boolean" || w[0] == "double" || w[0] == ">" || w[0] == "]")
                && abstract_token(w[1]) == "I"
                && matches!(w[2], "=" | ";" | ":" | ",")
            {
                out.insert(w[1].to_string());
            }
        }
    }
    out
}

/// Rewrites the free names of `stmt` through `map`. A name after a `.` or before a `(` is a member or a call, and stays.
fn rewrite(stmt: &str, map: &dyn Fn(&str) -> Option<String>) -> String {
    let toks = tokens(stmt);
    let base = stmt.as_ptr() as usize;
    let mut out = String::new();
    let mut last = 0usize;
    for (i, t) in toks.iter().enumerate() {
        let (a, b) = (t.as_ptr() as usize - base, t.as_ptr() as usize - base + t.len());
        // `this.count` names a field of the class, so it is a free name too; `other.count` and `.count()` are not
        let after_dot = i >= 1 && toks[i - 1] == "." && !(i >= 2 && toks[i - 2] == "this");
        let free = abstract_token(t) == "I" && !t.starts_with(|c: char| c.is_ascii_uppercase()) && !after_dot && toks.get(i + 1).map_or(true, |n| *n != "(");
        if free {
            if let Some(new) = map(t) {
                out.push_str(&stmt[last..a]);
                out.push_str(&new);
                last = b;
            }
        }
    }
    out.push_str(&stmt[last..]);
    out
}

/// The template's statements adapted to the target (parameters by position, the derived property, other names by the ranker).
pub(super) fn adapt(tmpl: &Method, q: &Method, fields: &[(String, String)]) -> Vec<String> {
    let locals = declared_locals(&tmpl.statements);
    let (tp, qp) = (property_of(&tmpl.name), property_of(&q.name));
    // what the target offers a free name, as popup rows for the ranker
    let offers: Vec<(String, String, &str)> = q.params.iter().map(|(t, n)| (n.clone(), t.clone(), "local")).chain(fields.iter().map(|(t, n)| (n.clone(), t.clone(), "field"))).collect();
    let map = |name: &str| -> Option<String> {
        if locals.contains(name) {
            return None;
        }
        if let Some(i) = tmpl.params.iter().position(|(_, n)| n == name) {
            return q.params.get(i).map(|(_, n)| n.clone()).filter(|n| n != name);
        }
        if tp.as_deref() == Some(name) {
            if let Some(p) = &qp {
                return Some(p.clone()).filter(|p| p != name);
            }
        }
        if offers.is_empty() {
            return None;
        }
        let rows: Vec<Row> = offers.iter().enumerate().map(|(rank, (n, t, k))| Row { name: n, kind: k, detail: t, deprecated: false, recency: 0, rank }).collect();
        let best = flc_rank::order("", name, &rows, 0)[0];
        let (n, ..) = &offers[best];
        let shared = subwords(n).iter().any(|w| subwords(name).contains(w));
        (shared && n != name).then(|| n.clone())
    };
    tmpl.statements.iter().map(|s| rewrite(s, &map)).collect()
}

pub(super) fn body_text(stmts: &[String], inner: &str, outer: &str) -> String {
    format!("{{\n{}\n{outer}}}", stmts.iter().map(|s| format!("{inner}{s}")).collect::<Vec<_>>().join("\n"))
}

type ErrSet = HashSet<(String, String)>;

pub(super) fn body_errors(svc: &IndexService, file: &str, source: &str, from: usize, to: usize) -> ErrSet {
    svc.validate_java(file, source, true)
        .into_iter()
        .filter(|d| d.severity == "error" && d.start >= from && d.start <= to)
        .map(|d| (d.code, source.get(d.start..d.end).unwrap_or("").to_string()))
        .collect()
}

#[derive(Default, Clone)]
struct Cell {
    n: usize,
    produced: usize,
    compiles: usize,
    exact: usize,
    form: usize,
    any_exact: usize,
    lines: usize,
    lines_total: usize,
}

#[test]
#[ignore = "needs the public corpus and a project index: see the module docs"]
fn m4_fill_and_compile() {
    let wanted = env_num("BENNU_MLB_N", 200);
    let only = env("BENNU_MLB_ONLY");
    let svc = IndexService::global();
    let mut train = Corpus::empty();
    let mut file_ids = 0usize;
    for t in ["commons-io", "commons-collections", "mockito", "junit5", "jackson-databind"] {
        let root = super::m3_rank::assemble_training(t);
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        for f in files {
            file_ids += 1;
            for m in methods_of(&std::fs::read_to_string(&f).unwrap_or_default(), file_ids) {
                train.add(m);
            }
        }
    }
    let mut cells: BTreeMap<&'static str, Cell> = BTreeMap::new();
    let mut shown = 0;
    for spec in PROJECTS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)) {
        let (root, held) = assemble(spec);
        open_and_wait(&root);
        let mut own = Corpus { methods: train.methods.clone(), skeletons: train.skeletons.clone(), bodies: train.bodies.clone(), inverted: train.inverted.clone(), sizes: train.sizes.clone() };
        let mut queries: Vec<(usize, Method)> = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for m in methods_of(&h.text, 1_000_000 + hi) {
                queries.push((hi, m.clone()));
                own.add(m);
            }
        }
        let stride = (queries.len() / wanted.max(1)).max(1);
        for (hi, q) in queries.iter().step_by(stride).take(wanted) {
            let h = &held[*hi];
            let file = h.path.to_string_lossy().into_owned();
            let fields = class_fields(&h.text, q.start);
            let truth = normalize(&q.statements.join(" ¦ "));
            let line_start = h.text[..q.start].rfind('\n').map_or(0, |p| p + 1);
            let outer: String = h.text[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let inner = format!("{outer}    ");
            let original_errors = body_errors(svc, &file, &h.text, q.body.0, q.body.1);
            let templates: Vec<usize> = own.top(q, 40).into_iter().filter(|&i| own.methods[i].params.len() == q.params.len()).take(5).collect();
            let c = cells.entry(class_of(q)).or_default();
            c.n += 1;
            c.lines_total += q.statements.len();
            if templates.is_empty() {
                continue;
            }
            c.produced += 1;
            let mut chosen: Option<(Vec<String>, bool)> = None;
            let mut any_exact = false;
            for &i in &templates {
                let stmts = adapt(&own.methods[i], q, &fields);
                any_exact |= normalize(&stmts.join(" ¦ ")) == truth;
                let body = body_text(&stmts, &inner, &outer);
                let candidate = format!("{}{}{}", &h.text[..q.body.0], body, &h.text[q.body.1..]);
                let errors = body_errors(svc, &file, &candidate, q.body.0, q.body.0 + body.len());
                let fine = errors.is_subset(&original_errors);
                if chosen.is_none() || (fine && !chosen.as_ref().unwrap().1) {
                    chosen = Some((stmts, fine));
                }
                if fine {
                    break;
                }
            }
            let (stmts, fine) = chosen.unwrap();
            let skeleton_of = |s: &[String]| skeleton(&Method { name: q.name.clone(), ret: String::new(), params: Vec::new(), statements: s.to_vec(), file: 0, start: 0, body: (0, 0) });
            c.compiles += usize::from(fine);
            c.exact += usize::from(normalize(&stmts.join(" ¦ ")) == truth);
            c.form += usize::from(skeleton_of(&stmts) == skeleton(q));
            c.any_exact += usize::from(any_exact);
            let normalized: Vec<String> = stmts.iter().map(|s| normalize(s)).collect();
            c.lines += q.statements.iter().filter(|s| normalized.contains(&normalize(s))).count();
            if env("BENNU_MLB_SHOW").is_some() && shown < 12 && fine {
                shown += 1;
                eprintln!("[{}] {}({})\n  want: {}\n  got : {}", class_of(q), q.name, q.params.iter().map(|p| p.1.clone()).collect::<Vec<_>>().join(", "), q.statements.join(" "), stmts.join(" "));
            }
        }
        svc.forget(&root.to_string_lossy());
    }
    let mut total = Cell::default();
    let mut out = String::from("## M4 — generating whole methods: retrieve, adapt, check\n\n| class | methods | produces something | compiles | exact | exact in form | exact in one of five | lines exact per method |\n|---|---|---|---|---|---|---|---|\n");
    let row = |name: &str, c: &Cell| format!("| {name} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.2} of {:.2} |\n", c.n, pct(c.produced, c.n), pct(c.compiles, c.n), pct(c.exact, c.n), pct(c.form, c.n), pct(c.any_exact, c.n), c.lines as f64 / c.n.max(1) as f64, c.lines_total as f64 / c.n.max(1) as f64);
    let mut rows: Vec<_> = cells.iter().collect();
    rows.sort_by(|a, b| b.1.exact.cmp(&a.1.exact).then(b.1.compiles.cmp(&a.1.compiles)));
    rows.sort_by(|a, b| (b.1.exact as f64 / b.1.n.max(1) as f64).total_cmp(&(a.1.exact as f64 / a.1.n.max(1) as f64)));
    for (k, c) in rows {
        out.push_str(&row(k, c));
        total.n += c.n; total.produced += c.produced; total.compiles += c.compiles; total.exact += c.exact; total.form += c.form; total.any_exact += c.any_exact; total.lines += c.lines; total.lines_total += c.lines_total;
    }
    out.push_str(&row("**all**", &total));
    let path = write_result("m4_fill_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(src: &str) -> Method {
        methods_of(src, 1).remove(0)
    }

    #[test]
    fn a_getter_gives_its_property_and_others_do_not() {
        assert_eq!(property_of("getName").as_deref(), Some("name"));
        assert_eq!(property_of("isActive").as_deref(), Some("active"));
        assert_eq!(property_of("setURL").as_deref(), Some("uRL"));
        assert_eq!(property_of("get"), None);
        assert_eq!(property_of("size"), None);
        assert_eq!(property_of("getaway"), None);
    }

    #[test]
    fn a_getter_is_adapted_to_another_property_and_a_setter_to_another_parameter() {
        let tmpl = method("class A { int getCount() { return count; } }");
        let q = method("class B { String getLabel() { return label; } }");
        let fields = vec![("String".to_string(), "label".to_string())];
        assert_eq!(adapt(&tmpl, &q, &fields), vec!["return label;".to_string()]);
        let tset = method("class A { void setCount(int c) { this.count = c; } }");
        let qset = method("class B { void setLabel(String value) { int z = 0; } }");
        assert_eq!(adapt(&tset, &qset, &fields), vec!["this.label = value;".to_string()]);
    }

    #[test]
    fn locals_calls_and_members_stay_where_they_are() {
        let tmpl = method("class A { int total(int a) { int sum = a; return sum + helper(sum); } }");
        let q = method("class B { int total(int b) { return 0; } }");
        let got = adapt(&tmpl, &q, &[]);
        assert_eq!(got, vec!["int sum = b;".to_string(), "return sum + helper(sum);".to_string()]);
    }

    #[test]
    fn the_fields_of_the_methods_class_are_found() {
        let src = "class A { private int n; String s, t; void f() { n = 1; } }";
        let m = method(src);
        assert_eq!(class_fields(src, m.start), vec![("int".into(), "n".into()), ("String".into(), "s".into()), ("String".into(), "t".into())]);
    }
}
