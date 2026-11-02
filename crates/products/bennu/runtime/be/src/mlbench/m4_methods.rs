//! M4 — can whole methods be written from the context without a neural model? What this measures is the **retrieval half**.
//!
//! For held-out methods with 1–8 statements, from name + signature it retrieves the most similar methods (by name sub-words
//! and signature types) of the project and of the public training projects, and asks: is the method's *skeleton* (the
//! sequence of statement forms, identifiers → `I`, literals → `L`) among the top k? is its body, verbatim, among them? how
//! many of its lines appear verbatim in the best match? The ceiling of "retrieve a skeleton, fill the holes" is the first one.
//!
//! NOT built here, so NOT measured: filling the holes with the type-driven enumerator ranked by the learned ranker, and the
//! compile check of the result. "Compiles", "exact after filling" and "exact lines per generated method" are therefore absent.
//!
//! `cargo test -p bennu-be mlbench::m4_methods -- --ignored --nocapture`  (`BENNU_MLB_N` methods per project, default 800)

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use super::common::*;
use super::m5_ngram::{abstract_token, tokens};

#[derive(Clone, Debug)]
pub(super) struct Method {
    pub name: String,
    pub ret: String,
    pub params: Vec<(String, String)>,
    pub statements: Vec<String>,
    pub file: usize,
    pub start: usize,
    /// Byte range of the body block, braces included.
    pub body: (usize, usize),
}

fn text<'a>(src: &'a str, n: &Node) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

/// Methods with 1–8 statements in `src`.
pub(super) fn methods_of(src: &str, file: usize) -> Vec<Method> {
    let Some(tree) = parse_java(src) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        let kids: Vec<Node> = n.children(&mut c).collect();
        if n.kind() == "method_declaration" {
            if let (Some(name), Some(body)) = (n.child_by_field_name("name"), n.child_by_field_name("body")) {
                let mut bc = body.walk();
                let stmts: Vec<String> = body.children(&mut bc).filter(|s| s.is_named() && !s.kind().ends_with("comment")).map(|s| text(src, &s).to_string()).collect();
                if (1..=8).contains(&stmts.len()) {
                    let mut params = Vec::new();
                    if let Some(list) = n.child_by_field_name("parameters") {
                        let mut pc = list.walk();
                        for p in list.children(&mut pc).filter(|p| p.kind() == "formal_parameter") {
                            let (ty, nm) = (p.child_by_field_name("type"), p.child_by_field_name("name"));
                            if let (Some(ty), Some(nm)) = (ty, nm) {
                                params.push((text(src, &ty).to_string(), text(src, &nm).to_string()));
                            }
                        }
                    }
                    out.push(Method { name: text(src, &name).to_string(), ret: n.child_by_field_name("type").map(|t| text(src, &t).to_string()).unwrap_or_default(), params, statements: stmts, file, start: n.start_byte(), body: (body.start_byte(), body.end_byte()) });
                }
            }
        }
        stack.extend(kids);
    }
    out
}

pub(super) fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn skeleton(m: &Method) -> String {
    m.statements.iter().map(|s| tokens(s).iter().map(|t| abstract_token(t)).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join(" ¦ ")
}

pub(super) fn body_key(m: &Method) -> String {
    m.statements.iter().map(|s| normalize(s)).collect::<Vec<_>>().join(" ¦ ")
}

pub(super) fn subwords(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch == '_' || !ch.is_alphanumeric() {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur).to_lowercase()) }
        } else if ch.is_uppercase() && cur.chars().last().is_some_and(|l| l.is_lowercase()) {
            out.push(std::mem::take(&mut cur).to_lowercase());
            cur.push(ch);
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() { out.push(cur.to_lowercase()) }
    out
}

/// The features a method is retrieved by: its name's sub-words (weight 2) and the simple names of its signature's types (1).
pub(super) fn features(m: &Method) -> Vec<(String, f32)> {
    let mut f: Vec<(String, f32)> = subwords(&m.name).into_iter().map(|w| (format!("n:{w}"), 2.0)).collect();
    let ty = |t: &str| format!("t:{}", t.split('<').next().unwrap_or(t).trim_end_matches("[]").rsplit('.').next().unwrap_or(""));
    f.push((ty(&m.ret), 1.0));
    f.extend(m.params.iter().map(|(t, _)| (ty(t), 1.0)));
    f.push((format!("a:{}", m.params.len()), 1.0));
    f
}

pub(super) fn class_of(m: &Method) -> &'static str {
    let first = subwords(&m.name).into_iter().next().unwrap_or_default();
    let one = m.statements.len() == 1;
    let s0 = m.statements.first().map(String::as_str).unwrap_or("");
    if m.params.is_empty() && one && ["get", "is", "has"].contains(&first.as_str()) && s0.starts_with("return ") && !s0.contains('(') {
        "getter"
    } else if m.params.len() == 1 && one && first == "set" && s0.contains('=') && !s0.contains('(') {
        "setter"
    } else if ["check", "validate", "verify", "assert", "require", "ensure", "is", "has", "can", "matches", "supports"].contains(&first.as_str()) && (m.ret == "boolean" || m.statements.iter().any(|s| s.contains("throw ") || s.starts_with("if"))) {
        "validation"
    } else if ["to", "from", "as", "of", "convert", "map", "parse", "value", "wrap", "unwrap", "format", "decode", "encode"].contains(&first.as_str()) {
        "conversion"
    } else if ["save", "find", "delete", "remove", "create", "update", "insert", "add", "put", "load", "store", "list", "get", "clear", "read", "write"].contains(&first.as_str()) && !(one && s0.starts_with("return ") && !s0.contains('(')) {
        "CRUD"
    } else if one && (s0.starts_with("return ") || s0.ends_with(';')) && s0.matches('(').count() == 1 && s0.contains('.') {
        "delegation"
    } else {
        "other"
    }
}

pub(super) struct Corpus {
    pub methods: Vec<Method>,
    pub skeletons: Vec<String>,
    pub bodies: Vec<String>,
    pub inverted: HashMap<String, Vec<(u32, f32)>>,
    pub sizes: Vec<f32>,
}

impl Corpus {
    pub(super) fn empty() -> Self {
        Corpus { methods: Vec::new(), skeletons: Vec::new(), bodies: Vec::new(), inverted: HashMap::new(), sizes: Vec::new() }
    }

    pub(super) fn add(&mut self, m: Method) {
        let id = self.methods.len() as u32;
        let feats = features(&m);
        self.sizes.push(feats.iter().map(|f| f.1).sum());
        for (f, w) in feats {
            self.inverted.entry(f).or_default().push((id, w));
        }
        self.skeletons.push(skeleton(&m));
        self.bodies.push(body_key(&m));
        self.methods.push(m);
    }

    /// Top `k` by weighted overlap over the union (Jaccard); the query itself (same file, same place) never answers.
    pub(super) fn top(&self, q: &Method, k: usize) -> Vec<usize> {
        let qf = features(q);
        let qsize: f32 = qf.iter().map(|f| f.1).sum();
        let mut score: HashMap<u32, f32> = HashMap::new();
        for (f, w) in &qf {
            for &(id, cw) in self.inverted.get(f).map(Vec::as_slice).unwrap_or(&[]) {
                *score.entry(id).or_default() += w.min(cw);
            }
        }
        let mut ranked: Vec<(usize, f32)> = score
            .into_iter()
            .filter(|&(id, _)| { let m = &self.methods[id as usize]; !(m.file == q.file && m.start == q.start) })
            .map(|(id, inter)| (id as usize, inter / (qsize + self.sizes[id as usize] - inter)))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.into_iter().take(k).map(|x| x.0).collect()
    }
}

pub(super) fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { rust_files(&p, out) } else if p.extension().is_some_and(|x| x == "java") { out.push(p) }
    }
}

#[derive(Default, Clone)]
struct Cell {
    n: usize,
    produced: usize,
    skel1: usize,
    skel5: usize,
    skel20: usize,
    body1: usize,
    body20: usize,
    lines: usize,
    lines_total: usize,
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m4_method_retrieval_ceiling() {
    let wanted = env_num("BENNU_MLB_N", 800);
    // the public training projects
    let mut train = Corpus { methods: Vec::new(), skeletons: Vec::new(), bodies: Vec::new(), inverted: HashMap::new(), sizes: Vec::new() };
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
    eprintln!("M4: {} public training methods", train.methods.len());
    let mut cells: BTreeMap<&'static str, Cell> = BTreeMap::new();
    for spec in PROJECTS {
        let (_, held) = assemble(spec);
        // the project's own methods join the corpus: a method may resemble a sibling
        let mut own = Corpus { methods: train.methods.clone(), skeletons: train.skeletons.clone(), bodies: train.bodies.clone(), inverted: train.inverted.clone(), sizes: train.sizes.clone() };
        let mut queries = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            let fid = 1_000_000 + hi;
            for m in methods_of(&h.text, fid) {
                queries.push(m.clone());
                own.add(m);
            }
        }
        let stride = (queries.len() / wanted.max(1)).max(1);
        for q in queries.iter().step_by(stride).take(wanted) {
            let top = own.top(q, 20);
            let (sk, bd) = (skeleton(q), body_key(q));
            let c = cells.entry(class_of(q)).or_default();
            c.n += 1;
            c.produced += usize::from(!top.is_empty());
            c.skel1 += usize::from(top.first().is_some_and(|&i| own.skeletons[i] == sk));
            c.skel5 += usize::from(top.iter().take(5).any(|&i| own.skeletons[i] == sk));
            c.skel20 += usize::from(top.iter().any(|&i| own.skeletons[i] == sk));
            c.body1 += usize::from(top.first().is_some_and(|&i| own.bodies[i] == bd));
            c.body20 += usize::from(top.iter().any(|&i| own.bodies[i] == bd));
            if let Some(&i) = top.first() {
                let best: Vec<String> = own.methods[i].statements.iter().map(|s| normalize(s)).collect();
                c.lines += q.statements.iter().filter(|s| best.contains(&normalize(s))).count();
            }
            c.lines_total += q.statements.len();
        }
    }
    let mut total = Cell::default();
    let mut out = String::from("## M4 — retrieval ceiling for whole methods (1–8 statements)\n\n| class | methods | retrieved something | skeleton top-1 | skeleton in top-5 | skeleton in top-20 (ceiling) | body verbatim top-1 | body verbatim in top-20 | lines verbatim in best match |\n|---|---|---|---|---|---|---|---|---|\n");
    let row = |name: &str, c: &Cell| format!("| {name} | {} | {:.1}% | {:.1}% | {:.1}% | **{:.1}%** | {:.1}% | {:.1}% | {:.1}% |\n", c.n, pct(c.produced, c.n), pct(c.skel1, c.n), pct(c.skel5, c.n), pct(c.skel20, c.n), pct(c.body1, c.n), pct(c.body20, c.n), pct(c.lines, c.lines_total));
    for (k, c) in &cells {
        out.push_str(&row(k, c));
        total.n += c.n; total.produced += c.produced; total.skel1 += c.skel1; total.skel5 += c.skel5; total.skel20 += c.skel20; total.body1 += c.body1; total.body20 += c.body20; total.lines += c.lines; total.lines_total += c.lines_total;
    }
    out.push_str(&row("**all**", &total));
    let path = write_result("m4_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "class A { private int n; public int getN() { return n; } void setN(int v) { this.n = v; } boolean isValid(String s) { if (s == null) { throw new IllegalStateException(); } return true; } }";

    #[test]
    fn methods_with_one_to_eight_statements_are_found_with_their_signature() {
        let ms = methods_of(SRC, 1);
        assert_eq!(ms.len(), 3);
        let get = ms.iter().find(|m| m.name == "getN").unwrap();
        assert_eq!((get.ret.as_str(), get.params.len(), get.statements.len()), ("int", 0, 1));
    }

    #[test]
    fn the_skeleton_forgets_names_and_literals_and_the_classes_are_told_apart() {
        let ms = methods_of(SRC, 1);
        let by = |n: &str| ms.iter().find(|m| m.name == n).unwrap();
        assert_eq!(skeleton(by("getN")), "return I ;");
        assert_eq!(class_of(by("getN")), "getter");
        assert_eq!(class_of(by("setN")), "setter");
        assert_eq!(class_of(by("isValid")), "validation");
    }

    #[test]
    fn a_method_is_retrieved_by_its_name_words_and_never_by_itself() {
        let mut c = Corpus { methods: Vec::new(), skeletons: Vec::new(), bodies: Vec::new(), inverted: HashMap::new(), sizes: Vec::new() };
        let a = methods_of("class A { int getCount() { return count; } }", 1);
        let b = methods_of("class B { int getTotalCount() { return total; } String name() { return n; } }", 2);
        for m in a.iter().chain(&b) { c.add(m.clone()); }
        let top = c.top(&a[0], 5);
        assert!(!top.iter().any(|&i| c.methods[i].file == 1), "the query answered itself");
        assert_eq!(c.methods[top[0]].name, "getTotalCount");
    }
}
